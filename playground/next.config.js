import path from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

const basePath = process.env.NEXT_PUBLIC_BASE_PATH || ''

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'export',
  basePath,
  assetPrefix: basePath || undefined,
  outputFileTracingRoot: path.join(__dirname, '..'),
  images: {
    unoptimized: true,
  },
  typescript: {
    ignoreBuildErrors: false,
  },
  // Sandboxed hosts (dot.li) serve this static export from a CAR via a service
  // worker, and `monaco-editor` lazily `import()`s a chunk per language —
  // hundreds of files, any one of which 404s if the CAR doesn't surface it
  // (ChunkLoadError). Collapse the client build into a handful of chunks so the
  // whole bundle ships and loads reliably.
  webpack: (config, { webpack, isServer, dev }) => {
    if (!isServer && !dev) {
      config.optimization.runtimeChunk = false;
      config.optimization.splitChunks = false;
      config.plugins.push(
        new webpack.optimize.LimitChunkCountPlugin({ maxChunks: 1 }),
      );
    }
    return config;
  },
}

export default nextConfig
