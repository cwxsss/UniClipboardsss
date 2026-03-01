import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

const ALLOWED_ROUTES = ['/settings']

/**
 * Listen for `ui://navigate` events from the backend
 * and invoke the provided callback for whitelisted routes.
 */
export function useUINavigateListener(onNavigate: (route: string) => void) {
  useEffect(() => {
    const unlistenPromise = listen<string>('ui://navigate', event => {
      const route = event.payload
      if (ALLOWED_ROUTES.includes(route)) {
        onNavigate(route)
      } else {
        console.warn(`Blocked navigation to non-whitelisted route: ${route}`)
      }
    })

    return () => {
      unlistenPromise.then(unlisten => unlisten())
    }
  }, [onNavigate])
}
