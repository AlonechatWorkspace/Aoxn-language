/** @type {import('next').NextConfig} */
const nextConfig = {
  // Tauri serves the frontend from the filesystem, so there is no Node
  // server in production and no image optimiser, no API routes, and no
  // per-request anything. A static export is the whole deployment model.
  output: 'export',

  // Monaco is ~5 MB of JavaScript. It is the editor, so it is not worth
  // splitting — but it must not be pulled into the server render, where it
  // touches `window`. `next/dynamic` with `ssr: false` (in the editor
  // component) is what keeps it client-only.
  productionBrowserSourceMaps: false,
}

export default nextConfig