import react from '@vitejs/plugin-react'
import { defineConfig, loadEnv } from 'vite'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const gatewayUrl = env.VITE_KAMISH_GATEWAY_URL || 'http://127.0.0.1:3001'

  return {
    plugins: [react()],
    server: {
      port: 5173,
      host: '0.0.0.0',
      proxy: {
        '/api': {
          target: gatewayUrl,
          changeOrigin: true,
        },
        '/metrics': {
          target: gatewayUrl,
          changeOrigin: true,
        },
      },
    },
  }
})
