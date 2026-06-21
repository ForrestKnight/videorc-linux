import { describe, expect, it } from 'vitest'

import { createSafeConsole, isBrokenPipeError, type ConsoleLike } from './safe-console'

describe('safe-console', () => {
  it('swallows broken pipe writes from console methods', () => {
    const logger = createSafeConsole(
      consoleThatThrows(Object.assign(new Error('write EPIPE'), { code: 'EPIPE' }))
    )

    expect(() => logger.log('[backend:info] hello')).not.toThrow()
    expect(() => logger.warn('[backend:warn] hello')).not.toThrow()
    expect(() => logger.error('[backend:error] hello')).not.toThrow()
  })

  it('rethrows non-pipe console write failures', () => {
    const logger = createSafeConsole(consoleThatThrows(new Error('unexpected console failure')))

    expect(() => logger.log('hello')).toThrow(/unexpected console failure/)
  })

  it('forwards arguments to the wrapped console method', () => {
    const writes: unknown[][] = []
    const logger = createSafeConsole({
      log: (...args) => writes.push(args),
      warn: (...args) => writes.push(args),
      error: (...args) => writes.push(args)
    })

    logger.log('one', { two: true })

    expect(writes).toEqual([['one', { two: true }]])
  })

  it('recognizes stream-destroyed write errors as broken pipes', () => {
    expect(isBrokenPipeError({ code: 'ERR_STREAM_DESTROYED' })).toBe(true)
    expect(isBrokenPipeError({ code: 'ERR_STREAM_WRITE_AFTER_END' })).toBe(true)
    expect(isBrokenPipeError({ code: 'EACCES' })).toBe(false)
  })
})

function consoleThatThrows(error: Error): ConsoleLike {
  return {
    log: () => {
      throw error
    },
    warn: () => {
      throw error
    },
    error: () => {
      throw error
    }
  }
}
