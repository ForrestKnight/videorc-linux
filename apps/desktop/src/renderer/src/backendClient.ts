import type {
  BackendConnection,
  ClientCommand,
  ServerEvent,
  ServerResponse
} from '../../shared/backend'

type PendingRequest = {
  resolve: (value: unknown) => void
  reject: (reason?: unknown) => void
}

type EventHandler = (payload: unknown) => void

export class BackendClient {
  private ws: WebSocket | null = null
  private pending = new Map<string, PendingRequest>()
  private handlers = new Map<string, Set<EventHandler>>()
  private requestCounter = 0

  constructor(private readonly connection: BackendConnection) {}

  connect(): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) {
      return Promise.resolve()
    }

    return new Promise((resolve, reject) => {
      const url = `ws://${this.connection.host}:${this.connection.port}/ws?token=${encodeURIComponent(
        this.connection.token
      )}`
      const ws = new WebSocket(url)
      this.ws = ws

      ws.onopen = () => resolve()
      ws.onerror = () => reject(new Error('Could not connect to the Rust backend.'))
      ws.onmessage = (event) => this.handleMessage(event.data)
      ws.onclose = () => {
        for (const request of this.pending.values()) {
          request.reject(new Error('Backend connection closed.'))
        }
        this.pending.clear()
        this.emit('connection.closed', null)
      }
    })
  }

  close(): void {
    this.ws?.close()
    this.ws = null
  }

  request<TPayload>(method: string, params?: unknown): Promise<TPayload> {
    const ws = this.ws
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error('Backend WebSocket is not connected.'))
    }

    const id = `renderer-${Date.now()}-${++this.requestCounter}`
    const command: ClientCommand = { id, method, params }

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject })
      ws.send(JSON.stringify(command))
    })
  }

  on(event: string, handler: EventHandler): () => void {
    const handlers = this.handlers.get(event) ?? new Set<EventHandler>()
    handlers.add(handler)
    this.handlers.set(event, handlers)

    return () => {
      handlers.delete(handler)
      if (handlers.size === 0) {
        this.handlers.delete(event)
      }
    }
  }

  private handleMessage(raw: string): void {
    let parsed: ServerResponse | ServerEvent
    try {
      parsed = JSON.parse(raw) as ServerResponse | ServerEvent
    } catch {
      this.emit('error', { message: 'Backend sent invalid JSON.' })
      return
    }

    if ('id' in parsed) {
      const pending = this.pending.get(parsed.id)
      if (!pending) {
        return
      }

      this.pending.delete(parsed.id)
      if (parsed.ok) {
        pending.resolve(parsed.payload)
      } else {
        pending.reject(new Error(parsed.error?.message ?? 'Backend request failed.'))
      }
      return
    }

    this.emit(parsed.event, parsed.payload)
  }

  private emit(event: string, payload: unknown): void {
    const handlers = this.handlers.get(event)
    if (!handlers) {
      return
    }

    for (const handler of handlers) {
      handler(payload)
    }
  }
}
