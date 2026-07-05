import React, { useEffect, useState, type ReactElement } from 'react'
import ReactDOM from 'react-dom/client'

import { CommentsReader } from '@/components/comments-reader'
import { AppErrorBoundary } from '@/components/error-boundary'
import type { LiveChatMessage, LiveChatSnapshot } from '@/lib/backend'
import { emptyLiveChatSnapshot } from '@/lib/live-chat-view'
import '@/styles.css'

// Long-lived second window: drop React's dev perf-track measures, which buffer
// outside the V8 heap and leak over time (see videorc-react-dev-perf-track-leak).
if (import.meta.env.DEV && localStorage.getItem('videorc.reactPerfTrack') !== '1') {
  const nativeMeasure = performance.measure.bind(performance)
  performance.measure = (
    name: string,
    startOrOptions?: string | PerformanceMeasureOptions,
    endMark?: string
  ): PerformanceMeasure => {
    const detail =
      typeof startOrOptions === 'object' && startOrOptions !== null ? startOrOptions.detail : null
    if (detail && typeof detail === 'object' && 'devtools' in detail) {
      return undefined as unknown as PerformanceMeasure
    }
    return nativeMeasure(name, startOrOptions, endMark)
  }
}

// The window's data comes from the main renderer through the main-process relay
// (C3): seed from the cached snapshot, then follow live pushes; Clear routes back.
function CommentsWindowApp(): ReactElement {
  const [snapshot, setSnapshot] = useState<LiveChatSnapshot>(() =>
    emptyLiveChatSnapshot(new Date().toISOString())
  )
  const [alwaysOnTop, setAlwaysOnTop] = useState(false)
  const [highlightedId, setHighlightedId] = useState<string | null>(null)
  useEffect(() => {
    void window.videorc
      ?.getCommentsSnapshot?.()
      .then((initial) => initial && setSnapshot(initial))
      .catch(() => {})
    void window.videorc
      ?.getCommentsWindowState?.()
      .then((state) => state && setAlwaysOnTop(state.alwaysOnTop))
      .catch(() => {})
    const offSnapshot = window.videorc?.onCommentsSnapshot?.((next) => setSnapshot(next))
    const offState = window.videorc?.onCommentsWindowState?.((state) =>
      setAlwaysOnTop(state.alwaysOnTop)
    )
    // Which comment is on stream: seeded + followed via the main-process relay
    // (the main renderer owns the highlight lifecycle).
    void window.videorc
      ?.getCommentHighlightState?.()
      .then((state) => state && setHighlightedId(state.messageId))
      .catch(() => {})
    const offHighlight = window.videorc?.onCommentHighlightState?.((state) =>
      setHighlightedId(state.messageId)
    )
    return () => {
      offSnapshot?.()
      offState?.()
      offHighlight?.()
    }
  }, [])
  return (
    <CommentsReader
      snapshot={snapshot}
      alwaysOnTop={alwaysOnTop}
      highlightedId={highlightedId}
      onClear={() => void window.videorc?.clearComments?.()}
      onHighlight={(message: LiveChatMessage) =>
        void window.videorc?.sendCommentHighlight?.(message)
      }
      onToggleAlwaysOnTop={() => void window.videorc?.setCommentsWindowAlwaysOnTop?.(!alwaysOnTop)}
    />
  )
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AppErrorBoundary>
      <CommentsWindowApp />
    </AppErrorBoundary>
  </React.StrictMode>
)
