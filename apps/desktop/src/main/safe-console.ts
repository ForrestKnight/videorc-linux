export interface ConsoleLike {
  log: (...args: unknown[]) => void
  warn: (...args: unknown[]) => void
  error: (...args: unknown[]) => void
}

export const safeConsole = createSafeConsole(console)

export function createSafeConsole(consoleLike: ConsoleLike): ConsoleLike {
  return {
    log: (...args) => writeSafely(consoleLike, consoleLike.log, args),
    warn: (...args) => writeSafely(consoleLike, consoleLike.warn, args),
    error: (...args) => writeSafely(consoleLike, consoleLike.error, args)
  }
}

export function isBrokenPipeError(error: unknown): boolean {
  const code = error && typeof error === 'object' ? (error as { code?: unknown }).code : undefined
  return (
    code === 'EPIPE' || code === 'ERR_STREAM_DESTROYED' || code === 'ERR_STREAM_WRITE_AFTER_END'
  )
}

function writeSafely(
  receiver: ConsoleLike,
  method: (...args: unknown[]) => void,
  args: unknown[]
): void {
  try {
    method.apply(receiver, args)
  } catch (error) {
    if (!isBrokenPipeError(error)) {
      throw error
    }
  }
}
