import { useCallback, useEffect, useRef, useState } from 'react'
import { useDispatch } from 'react-redux'
import { diffPeerSnapshots, type PeerSnapshotPeer } from '@/api/daemon/events'
import { getP2PPeers } from '@/api/daemon/pairing'
import { daemonWs } from '@/lib/daemon-ws'
import type { AppDispatch } from '@/store'
import {
  clearDiscoveredPeers,
  setDiscoveredPeers,
  updateDiscoveredPeerDeviceName,
  type DiscoveredPeer,
} from '@/store/slices/devicesSlice'

/**
 * Scanning state machine:
 *   'scanning'   -- initial state, waiting for devices or timeout
 *   'hasDevices' -- at least one device is in the list
 *   'empty'      -- 10s timeout elapsed and no devices found
 */
export type ScanPhase = 'scanning' | 'hasDevices' | 'empty'

export interface UseDeviceDiscoveryOptions {
  onError?: (error: Error) => void
}

export function useDeviceDiscovery(
  active: boolean,
  options?: UseDeviceDiscoveryOptions
): { scanPhase: ScanPhase; resetScan: () => void } {
  const dispatch = useDispatch<AppDispatch>()
  const [scanPhase, setScanPhase] = useState<ScanPhase>('scanning')
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Store onError in a ref so the main effect does not re-subscribe when the
  // callback identity changes between renders.
  const onErrorRef = useRef(options?.onError)

  // Sync ref in an effect (not during render, per react-hooks/refs rule)
  useEffect(() => {
    onErrorRef.current = options?.onError
  })

  /** Start (or restart) the 10-second empty-state timeout. */
  const startTimeout = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current)
    }
    timeoutRef.current = setTimeout(() => {
      setScanPhase(prev => (prev === 'scanning' ? 'empty' : prev))
    }, 10_000)
  }, [])

  /** Fetch the current peer list and populate Redux state. */
  const loadPeers = useCallback(async () => {
    try {
      const list = await getP2PPeers()
      const discovered: DiscoveredPeer[] = list.map(p => ({
        id: p.peerId,
        deviceName: p.deviceName ?? null,
        device_type: 'desktop',
      }))
      dispatch(setDiscoveredPeers(discovered))
      if (discovered.length > 0) {
        setScanPhase('hasDevices')
      }
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err))
      onErrorRef.current?.(error)
      // Do NOT transition to 'empty' on fetch error -- timeout handles that
      setScanPhase('scanning')
    }
  }, [dispatch])

  /** Public API: reset to scanning state and re-fetch. */
  const resetScan = useCallback(() => {
    dispatch(clearDiscoveredPeers())
    setScanPhase('scanning')
    startTimeout()
    void loadPeers()
  }, [dispatch, startTimeout, loadPeers])

  useEffect(() => {
    if (!active) {
      // Deactivation reset: clear stale data so re-entry starts fresh
      dispatch(clearDiscoveredPeers())
      setScanPhase('scanning')
      return
    }

    // Reset state on entry so re-entry always starts clean
    dispatch(clearDiscoveredPeers())
    setScanPhase('scanning')

    // Start the 10-second timeout
    startTimeout()

    // Initial peer load
    void loadPeers()

    // Track known peers for diffPeerSnapshots to detect newly discovered/lost peers
    const knownPeers = new Map<string, { deviceName?: string | null }>()

    const handler = (event: { topic: string; eventType: string; payload: unknown }) => {
      if (event.topic !== 'peers') return

      if (event.eventType === 'peers.changed') {
        const payload = event.payload as { peers: PeerSnapshotPeer[] }
        const nextPeers: DiscoveredPeer[] = []
        diffPeerSnapshots(payload.peers, knownPeers, diffEvent => {
          if (diffEvent.discovered) {
            nextPeers.push({
              id: diffEvent.peerId,
              deviceName: diffEvent.deviceName ?? null,
              device_type: 'desktop',
            })
          }
        })
        if (nextPeers.length > 0) {
          // Use functional updater to avoid stale closure state.
          // Passing a function to setDiscoveredPeers merges new peers with existing Redux state.
          dispatch(setDiscoveredPeers((prev: DiscoveredPeer[]) => [...prev, ...nextPeers]))
        }
        setScanPhase(knownPeers.size > 0 ? 'hasDevices' : 'empty')
        return
      }

      if (event.eventType === 'peers.nameUpdated') {
        const payload = event.payload as { peerId: string; deviceName: string }
        dispatch(
          updateDiscoveredPeerDeviceName({ peerId: payload.peerId, deviceName: payload.deviceName })
        )
        return
      }

      if (event.eventType === 'peers.connectionChanged') {
        // Connection state consumed elsewhere; discovery list stays stable.
      }
    }

    const unsubscribe = daemonWs.subscribe(['peers'], handler)

    return () => {
      dispatch(clearDiscoveredPeers())
      setScanPhase('scanning')
      knownPeers.clear()
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current)
        timeoutRef.current = null
      }
      unsubscribe()
    }
  }, [active, startTimeout, loadPeers, dispatch])

  return { scanPhase, resetScan }
}
