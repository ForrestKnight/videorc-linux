import type { VideogreApi } from '../../shared/backend'

declare global {
  interface Window {
    videogre: VideogreApi
  }
}

export {}
