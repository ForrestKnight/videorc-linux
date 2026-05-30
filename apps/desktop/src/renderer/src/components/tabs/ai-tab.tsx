import { Brain, DownloadSimple, Lightning, ShieldCheck, Sparkle } from '@phosphor-icons/react'
import { useEffect, type ReactElement, type ReactNode } from 'react'

import { PanelSection } from '@/components/panel-section'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from '@/components/ui/collapsible'
import { Empty, EmptyDescription, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { Field, FieldContent, FieldDescription, FieldLabel } from '@/components/ui/field'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { useStudio } from '@/hooks/use-studio'
import type { SessionSummary } from '@/lib/backend'
import { artifactChapters, artifactField, artifactText, dayLabel, latestArtifact } from '@/lib/format'

export function AiTab({
  selectedSessionId,
  setSelectedSessionId
}: {
  selectedSessionId: string | null
  setSelectedSessionId: (id: string | null) => void
}): ReactElement {
  const { sessions, aiConsent, setAiConsent, runAiWorkflow, exportPublishPack, aiRunningSessionId, exportRunningSessionId } =
    useStudio()

  useEffect(() => {
    if (!selectedSessionId && sessions.length > 0) {
      setSelectedSessionId(sessions[0].id)
    }
  }, [selectedSessionId, sessions, setSelectedSessionId])

  const selected = sessions.find((session) => session.id === selectedSessionId) ?? null

  if (sessions.length === 0) {
    return (
      <PanelSection icon={Sparkle} title="AI workflow">
        <Empty className="py-10">
          <EmptyMedia variant="icon">
            <Brain weight="duotone" />
          </EmptyMedia>
          <EmptyTitle>No sessions to analyze</EmptyTitle>
          <EmptyDescription>Record a session first, then run transcript, summary, and chapters here.</EmptyDescription>
        </Empty>
      </PanelSection>
    )
  }

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)]">
      <div className="flex flex-col gap-4">
        <PanelSection description="Pick a recording, then run or review its AI artifacts." icon={Sparkle} title="Session">
          <Field>
            <FieldLabel htmlFor="ai-session">Recording</FieldLabel>
            <Select value={selectedSessionId ?? ''} onValueChange={(value) => setSelectedSessionId(value)}>
              <SelectTrigger className="w-full" id="ai-session">
                <SelectValue placeholder="Select a session" />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {sessions.map((session) => (
                    <SelectItem key={session.id} value={session.id}>
                      {session.title} · {dayLabel(session.startedAt)}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>

          {selected ? <SessionActions session={selected} /> : null}
        </PanelSection>

        <PanelSection icon={ShieldCheck} title="Cloud AI consent">
          <Alert variant="warning">
            <ShieldCheck weight="fill" />
            <AlertTitle>Recordings stay local by default</AlertTitle>
            <AlertDescription>
              Without consent, Videogre only extracts local audio. Uses OPENAI_API_KEY when present; artifacts are stored
              locally with each session.
            </AlertDescription>
          </Alert>
          <Field orientation="horizontal">
            <FieldContent>
              <FieldLabel htmlFor="ai-consent">Allow cloud upload</FieldLabel>
              <FieldDescription>Upload extracted audio and transcript for summaries and chapters.</FieldDescription>
            </FieldContent>
            <Switch checked={aiConsent} id="ai-consent" onCheckedChange={setAiConsent} />
          </Field>
        </PanelSection>
      </div>

      <PanelSection icon={Brain} title="Publish pack">
        {selected ? (
          <ArtifactView session={selected} />
        ) : (
          <Empty className="border-0 py-6">
            <EmptyTitle>No session selected</EmptyTitle>
          </Empty>
        )}
      </PanelSection>
    </div>
  )

  function SessionActions({ session }: { session: SessionSummary }): ReactElement {
    const canRunAi = Boolean(session.status === 'completed' && session.outputPath)
    const canExportPublishPack = session.aiArtifacts.some(
      (artifact) => artifact.status === 'ready' && artifact.kind !== 'audio-extract'
    )
    const aiRunning = aiRunningSessionId === session.id
    const exportRunning = exportRunningSessionId === session.id

    return (
      <div className="flex flex-wrap gap-2">
        <Button disabled={!canRunAi || aiRunning} onClick={() => runAiWorkflow(session.id)}>
          <Lightning data-icon="inline-start" weight="fill" />
          {aiRunning ? 'Running…' : 'Run AI workflow'}
        </Button>
        <Button
          disabled={!canExportPublishPack || exportRunning}
          variant="outline"
          onClick={() => exportPublishPack(session.id)}
        >
          <DownloadSimple data-icon="inline-start" />
          {exportRunning ? 'Exporting…' : 'Export pack'}
        </Button>
      </div>
    )
  }
}

function ArtifactView({ session }: { session: SessionSummary }): ReactElement {
  const titleDescription = latestArtifact(session, 'title-description')
  const transcript = latestArtifact(session, 'transcript')
  const summary = latestArtifact(session, 'summary')
  const chapters = latestArtifact(session, 'chapters')
  const chapterItems = chapters ? artifactChapters(chapters) : []
  const title = titleDescription ? artifactField(titleDescription, 'title') : ''
  const description = titleDescription ? artifactField(titleDescription, 'description') : ''

  if (!session.aiArtifacts.length) {
    return (
      <Empty className="border-0 py-6">
        <EmptyTitle>No artifacts yet</EmptyTitle>
        <EmptyDescription>Run the AI workflow to generate transcript, summary, and chapters.</EmptyDescription>
      </Empty>
    )
  }

  return (
    <ScrollArea className="h-[calc(100vh-15rem)] pr-3">
      <div className="flex flex-col gap-2">
        {title || description ? (
          <ArtifactSection defaultOpen title="Title & description">
            {title ? <p className="font-medium">{title}</p> : null}
            {description ? <p className="text-sm text-muted-foreground">{description}</p> : null}
          </ArtifactSection>
        ) : null}
        {summary ? (
          <ArtifactSection defaultOpen title="Summary">
            <p className="text-sm whitespace-pre-line">{artifactText(summary)}</p>
          </ArtifactSection>
        ) : null}
        {chapterItems.length ? (
          <ArtifactSection title="Chapters">
            <ol className="flex flex-col gap-1.5">
              {chapterItems.map((chapter) => (
                <li className="flex gap-3 text-sm" key={`${chapter.timestamp}-${chapter.title}`}>
                  <time className="font-mono text-xs text-muted-foreground tabular-nums">{chapter.timestamp}</time>
                  <span>{chapter.title}</span>
                </li>
              ))}
            </ol>
          </ArtifactSection>
        ) : null}
        {transcript ? (
          <ArtifactSection title="Transcript">
            <p className="text-sm whitespace-pre-line text-muted-foreground">{artifactText(transcript)}</p>
          </ArtifactSection>
        ) : null}
      </div>
    </ScrollArea>
  )
}

function ArtifactSection({
  title,
  defaultOpen = false,
  children
}: {
  title: string
  defaultOpen?: boolean
  children: ReactNode
}): ReactElement {
  return (
    <Collapsible className="rounded-xl border bg-card" defaultOpen={defaultOpen}>
      <CollapsibleTrigger className="flex w-full items-center justify-between px-3 py-2.5 text-sm font-medium">
        {title}
      </CollapsibleTrigger>
      <CollapsibleContent className="flex flex-col gap-2 px-3 pb-3">{children}</CollapsibleContent>
    </Collapsible>
  )
}
