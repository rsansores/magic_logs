// The README's pictures of the demo: serves it, and has a headless Chromium
// (CHROMIUM, or `chromium` on the PATH) photograph it at each address.
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { createServer } from 'vite'

const run = promisify(execFile)
const HOUR = 3_600_000
const now = Date.now()
const incident = new URLSearchParams({
  window: 'custom',
  from: new Date(Math.floor((now - 3 * HOUR) / HOUR) * HOUR).toISOString(),
  to: new Date(now).toISOString(),
  level: 'error,critical',
  view: 'grouped',
})
const shots = {
  'images/explorer.png': '/',
  'images/explorer-zoomed.png': `/?${incident}`,
}

const server = await createServer({ server: { port: 5197, strictPort: true }, logLevel: 'error' })
await server.listen()
try {
  for (const [file, path] of Object.entries(shots)) {
    await run(process.env.CHROMIUM ?? 'chromium', [
      '--headless',
      '--hide-scrollbars',
      '--force-device-scale-factor=2',
      '--window-size=1440,900',
      '--virtual-time-budget=3000',
      `--screenshot=${file}`,
      `http://localhost:5197${path}`,
    ])
    console.log(`wrote ${file}`)
  }
} finally {
  await server.close()
}
