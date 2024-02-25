import {resolve} from 'path'
import {defineConfig, externalizeDepsPlugin} from 'electron-vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
    main: {
        plugins: [externalizeDepsPlugin()],
        build: {
            lib: {
                entry: 'src/main/main.ts'
            },
            sourcemap: true
        }
    },
    preload: {
        plugins: [externalizeDepsPlugin()],
        build: {
            lib: {
                entry: 'src/preload/preload.ts'
            },
            sourcemap: true
        }
    },
    renderer: {
        resolve: {
            alias: {
                '@renderer': resolve('src/renderer/src')
            }
        },
        plugins: [react()],
        build: {
            sourcemap: true,
            rollupOptions: {
                input: {
                    main: resolve('src/renderer/index.html'),
                    blank: resolve('src/renderer/blank.html'),
                    blankCSS: resolve('src/renderer/blank.css'),
                }
            }
        }
    }
})
