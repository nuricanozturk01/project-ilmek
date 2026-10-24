import { useEffect, useRef } from 'react'
import { onAppEvent, shell } from '../../shared/ipc.js'
import { useTabs } from './model.js'

/** The failure an application sees when the shell could not even relay its request. */
const RELAY_FAILED = {
  ok: false,
  error: {
    code: 'INTERNAL',
    message: 'The request could not be relayed.',
    retryable: true,
    outcomeUnknown: true,
  },
}

function isRequest(data: unknown): data is { ilmek: 1; id: string; request: unknown } {
  return (
    typeof data === 'object' &&
    data !== null &&
    'ilmek' in data &&
    data.ilmek === 1 &&
    'id' in data &&
    typeof data.id === 'string' &&
    'request' in data
  )
}

/**
 * Every live application tab is a sandboxed frame; only the active one is visible. The frame is
 * the only thing that identifies an application: a message counts only if its `source` is one of
 * these frames, and the reply goes back to that same frame. Pages cannot name another tab.
 */
export function Frames() {
  const { tabs, active } = useTabs()
  const frames = useRef(new Map<string, HTMLIFrameElement>())
  const labels = useRef(new Map<string, string>())

  useEffect(() => {
    labels.current = new Map(
      tabs.flatMap((tab) => (tab.label === null ? [] : [[tab.key, tab.label] as const])),
    )
  }, [tabs])

  useEffect(() => {
    function onMessage(event: MessageEvent) {
      if (!isRequest(event.data)) return
      for (const [key, frame] of frames.current) {
        if (frame.contentWindow !== event.source) continue
        const label = labels.current.get(key)
        if (label === undefined) return
        const { id, request } = event.data
        const target = frame.contentWindow
        if (target === null) return
        void shell.bridge(label, request).then(
          (response) => {
            target.postMessage({ ilmek: 1, id, response }, '*')
          },
          () => {
            target.postMessage({ ilmek: 1, id, response: RELAY_FAILED }, '*')
          },
        )
        return
      }
    }
    window.addEventListener('message', onMessage)
    return () => {
      window.removeEventListener('message', onMessage)
    }
  }, [])

  useEffect(() => {
    let stop: (() => void) | undefined
    let disposed = false
    void onAppEvent(({ label, json }) => {
      for (const [key, frame] of frames.current) {
        if (labels.current.get(key) === label) frame.contentWindow?.postMessage({ ilmek: 1, event: json }, '*')
      }
    }).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })
    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  return (
    <>
      {tabs.map((tab) =>
        tab.url === null ? null : (
          <iframe
            key={tab.key + (tab.label ?? '')}
            ref={(element) => {
              if (element === null) frames.current.delete(tab.key)
              else frames.current.set(tab.key, element)
            }}
            className="app-frame"
            title={tab.title ?? tab.address ?? ''}
            src={tab.url}
            sandbox="allow-scripts allow-forms"
            referrerPolicy="no-referrer"
            hidden={tab.key !== active.key}
          />
        ),
      )}
    </>
  )
}
