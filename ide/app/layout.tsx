import type { Metadata, Viewport } from 'next'
import './globals.css'

export const metadata: Metadata = {
  title: 'Aoxn IDE',
  description: 'Editor, diagnostics and build runner for the Aoxn language',
}

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
  // A desktop app has no pinch-zoom; a user ctrl-scrolling the workbench
  // would otherwise zoom the whole UI.
  maximumScale: 1,
  userScalable: false,
}

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" data-theme="dark">
      <body>{children}</body>
    </html>
  )
}