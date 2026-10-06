import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({ plugins: [react()], worker: { format: 'es', plugins: () => [{
  name: 'pdf-worker-exports',
  // PDF.js also imports this entry on WebViews that need its fake-worker fallback.
  options: options => ({ ...options, preserveEntrySignatures: 'strict' }),
}] }, server: { port: 1420, strictPort: true }, clearScreen: false });
