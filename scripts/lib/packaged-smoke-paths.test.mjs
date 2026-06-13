import assert from 'node:assert/strict'
import { describe, it } from 'node:test'

import {
  assertPackagedSmokePlatform,
  bundledFfmpegPathForPackagedApp,
  defaultPackagedAppExecutable
} from './packaged-smoke-paths.mjs'

describe('packaged smoke paths', () => {
  it('resolves the default macOS packaged executable and bundled FFmpeg', () => {
    const executable = defaultPackagedAppExecutable({ repoRoot: '/repo', platform: 'darwin' })

    assert.equal(executable, '/repo/apps/desktop/release/mac-arm64/Videorc.app/Contents/MacOS/Videorc')
    assert.equal(
      bundledFfmpegPathForPackagedApp({ appExecutable: executable, platform: 'darwin' }),
      '/repo/apps/desktop/release/mac-arm64/Videorc.app/Contents/Resources/ffmpeg/bin/ffmpeg'
    )
  })

  it('resolves the default Windows packaged executable and bundled FFmpeg', () => {
    const executable = defaultPackagedAppExecutable({ repoRoot: 'C:/repo', platform: 'win32' })

    assert.match(executable, /C:\/repo\/apps\/desktop\/release\/win-unpacked\/Videorc\.exe$/)
    assert.match(
      bundledFfmpegPathForPackagedApp({ appExecutable: executable, platform: 'win32' }),
      /C:\/repo\/apps\/desktop\/release\/win-unpacked\/resources\/ffmpeg\/bin\/ffmpeg\.exe$/
    )
  })

  it('rejects unsupported packaged smoke platforms', () => {
    assert.throws(() => assertPackagedSmokePlatform('linux'), /supports macOS and Windows/)
    assert.throws(() => defaultPackagedAppExecutable({ repoRoot: '/repo', platform: 'linux' }), /does not support linux/)
  })
})
