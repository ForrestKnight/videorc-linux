import { Moon, Sun } from '@phosphor-icons/react'
import { useTheme } from 'next-themes'
import { useEffect, useState, type ReactElement } from 'react'

import { Button } from '@/components/ui/button'

export function ThemeToggle(): ReactElement {
  const { resolvedTheme, setTheme } = useTheme()
  const [mounted, setMounted] = useState(false)

  useEffect(() => {
    setMounted(true)
  }, [])

  const isDark = resolvedTheme === 'dark'

  return (
    <Button
      aria-label="Toggle color theme"
      size="icon"
      title={isDark ? 'Switch to light theme' : 'Switch to dark theme'}
      variant="ghost"
      onClick={() => setTheme(isDark ? 'light' : 'dark')}
    >
      {mounted && isDark ? <Moon weight="fill" /> : <Sun weight="fill" />}
    </Button>
  )
}
