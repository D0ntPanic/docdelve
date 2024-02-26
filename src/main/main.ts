import {
    app,
    ipcMain,
    protocol,
    nativeTheme,
    BrowserWindow,
    Menu,
    MenuItemConstructorOptions
} from 'electron';
import * as path from 'path';
import {optimizer, is} from '@electron-toolkit/utils';
import {ItemPath, SearchParameters, SearchResult, Theme, ItemContents, ChestPath} from "../../docdelve_ffi";

// Load native module. Not directly to shut up the linter (this cannot be an import statement).
const ffiResolve = () => {
    return require('../../docdelve_ffi')
};
const ffi = ffiResolve();

// Load the documentation database on startup
const db = new ffi.Database();

let activeWindow: BrowserWindow | null = null;

app.setName('docdelve');
app.setAboutPanelOptions({
    applicationName: 'Doc Delve',
    applicationVersion: app.getVersion(),
    website: 'https://docdelve.app',
    iconPath: './images/docdelve_512x512.png'
});

// Helper function to make IPC APIs that can only be called from scripts that ship with
// the application.
function appLocalAPI(name: string, handler: (...args: any[]) => any) {
    ipcMain.handle(name, (event, ...args) => {
        const senderURL = new URL(event.senderFrame.url);
        const isLocal = senderURL.protocol === 'file:' || (is.dev && senderURL.hostname === 'localhost');
        if (!isLocal)
            throw new Error("Calling app local API from an invalid context");
        return handler(...args);
    });
}

// Helper function to make IPC APIs that can only be called from scripts that ship with
// the application.
/*function appWindowLocalAPI(window: BrowserWindow, name: string, handler: (...args: any[]) => any) {
    window.webContents.ipc.handle(name, (event, ...args) => {
        const senderURL = new URL(event.senderFrame.url);
        const isLocal = senderURL.protocol === 'file:' || (is.dev && senderURL.hostname === 'localhost');
        if (!isLocal)
            throw new Error("Calling app local API from an invalid context");
        return handler(...args);
    });
}*/

function createWindow() {
    let backgroundColor = "#242424";
    if (process.platform === 'darwin') {
        backgroundColor = "#00000000";
    }

    // Create the main window and load the starting page of the application
    const window = new BrowserWindow({
        width: 1024,
        height: 700,
        titleBarStyle: 'hidden',
        titleBarOverlay: {height: 52},
        show: false,
        vibrancy: "sidebar",
        backgroundColor: backgroundColor,
        trafficLightPosition: {x: 19, y: 18},
        webPreferences: {
            preload: path.join(__dirname, '../preload/preload.js'),
            webviewTag: true
        }
    })

    let firstWindowShow = true;
    window.on('ready-to-show', () => {
        if (firstWindowShow) {
            window.show()
            window.webContents.focus()
            window.webContents.send('focus-search')
            firstWindowShow = false
        }
    });

    window.on('focus', () => {
        activeWindow = window
        window.webContents.send('window-active')
    })
    window.on('blur', () => window.webContents.send('window-inactive'))
    window.on('close', () => {
        if (activeWindow === window) {
            activeWindow = null
        }
    })

    window.webContents.setWindowOpenHandler((_details) => {
        return {action: 'deny'};
    })

    nativeTheme.on('updated', () => {
        window.webContents.send('theme-updated')
    });

    if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
        return window.loadURL(process.env['ELECTRON_RENDERER_URL'])
    } else {
        return window.loadFile(path.join(__dirname, '../renderer/index.html'))
    }
}

appLocalAPI('search', (path: ItemPath | null, query: string,
                       parameters: SearchParameters | null): Array<ExtendedSearchResult> => {
    return db.search(path, query, parameters).map((result: SearchResult): ExtendedSearchResult => {
        const chestTag = db.tagForIdentifier(result.path.identifier)
        return {
            result: result,
            chestTag: chestTag,
            items: db.itemsAtPath(result.path)
        }
    })
})

appLocalAPI('page-for-path', (identifier: string, url: string, path: ItemPath | null): OptionalItemPath => {
    const chestTag = db.tagForIdentifier(identifier)
    let result = db.itemForPath(identifier, url, path)
    let pageResult = db.pageForPath(identifier, url, path)
    if (result === null) {
        return {
            identifier: identifier,
            chestTag: chestTag,
            chestPath: null,
            chestPagePath: null,
        };
    } else if (pageResult === null || result.identifier !== pageResult.identifier) {
        return {
            identifier: result.identifier,
            chestTag: chestTag,
            chestPath: result.chestPath,
            chestPagePath: result.chestPath
        }
    } else {
        return {
            identifier: result.identifier,
            chestTag: chestTag,
            chestPath: result.chestPath,
            chestPagePath: pageResult.chestPath
        }
    }
})

appLocalAPI('item-contents-at-path', (path: ItemPath | ExtendedItemPath): ItemContents => {
    const contents = db.itemContentsAtPath({identifier: path.identifier, chestPath: path.chestPath});
    let bases = contents.bases.map((basePath: ChestPath): BaseItems => {
        const fullBasePath = {
            identifier: path.identifier,
            chestPath: basePath
        }
        return {
            path: fullBasePath,
            items: db.itemsAtPath(fullBasePath)
        }
    })
    return {
        chestItems: contents.chestItems,
        pageItems: contents.pageItems,
        bases: bases
    }
})

// Initialize main window on startup and close the application when the main window closes
app.whenReady().then(() => {
    // Create a protocol handler for accessing the documentation chests
    protocol.handle('docs', (request) => {
        const url = new URL(request.url);
        let contents: Buffer, status: number;
        let type: string | null = null;
        try {
            // Try to read the requested file from the documentation chest
            let path = url.pathname;
            contents = db.read(url.host, path, (nativeTheme.shouldUseDarkColors ? 'Dark' : 'Light') as Theme);
            status = 200;

            // Set correct MIME type for some common extensions. This is required to load some types of files.
            if (url.pathname.toLowerCase().endsWith('.svg')) {
                type = 'image/svg+xml';
            } else if (url.pathname.toLowerCase().endsWith('.png')) {
                type = 'image/png';
            } else if (url.pathname.toLowerCase().endsWith('.jpg') || url.pathname.toLowerCase().endsWith('.jpeg')) {
                type = 'image/jpeg';
            } else if (url.pathname.toLowerCase().endsWith('.html')) {
                type = 'text/html';
            } else if (url.pathname.toLowerCase().endsWith('.css')) {
                type = 'text/css';
            } else if (url.pathname.toLowerCase().endsWith('.js')) {
                type = 'text/javascript';
            } else if (url.pathname.toLowerCase().endsWith('.json')) {
                type = 'application/json';
            } else if (url.pathname.toLowerCase().endsWith('.txt')) {
                type = 'text/plain';
            }
        } catch (error: any) {
            // On error, return a 404 status and a simple page showing the error message
            contents = Buffer.from('<!DOCTYPE html>\n<html lang="en"><h1>Error</h1><p>Error while fetching ' +
                request.url + '</p><p>' + error.message + '</p></html>');
            type = "text/html";
            status = 404;
        }
        if (type !== null) {
            return new Response(contents, {status: status, headers: {'Content-Type': type}});
        }
        return new Response(contents, {status: status})
    })

    let menuTemplate: MenuItemConstructorOptions[] = [];
    if (process.platform === 'darwin') {
        menuTemplate.push({
            label: app.name,
            submenu: [
                {label: 'About Doc Delve', role: 'about'},
                {type: 'separator'},
                {role: 'services'},
                {type: 'separator'},
                {label: "Hide Dock Delve", role: 'hide'},
                {role: 'hideOthers'},
                {role: 'unhide'},
                {type: 'separator'},
                {label: "Quit Dock Delve", role: 'quit'}
            ]
        })
    }

    menuTemplate.push({
        label: 'Navigate',
        submenu: [
            {
                label: 'Search All...',
                accelerator: process.platform === 'darwin' ? 'Command+G' : 'Ctrl+G',
                click: () => {
                    if (activeWindow !== null) {
                        activeWindow.webContents.focus()
                        activeWindow.webContents.send('focus-search')
                    }
                }
            },
            {type: 'separator'},
            {
                label: 'Previous Page',
                accelerator: process.platform === 'darwin' ? 'Command+[' : 'Ctrl+[',
                click: () => {
                    if (activeWindow !== null) {
                        activeWindow.webContents.focus()
                        activeWindow.webContents.send('navigate-back')
                    }
                }
            },
            {
                label: 'Next Page',
                accelerator: process.platform === 'darwin' ? 'Command+]' : 'Ctrl+]',
                click: () => {
                    if (activeWindow !== null) {
                        activeWindow.webContents.focus()
                        activeWindow.webContents.send('navigate-forward')
                    }
                }
            }
        ]
    })

    menuTemplate.push({
        label: 'Window',
        submenu: [
            {
                label: 'New Window',
                accelerator: process.platform === 'darwin' ? 'Command+N' : 'Ctrl+N',
                click: async () => {
                    return createWindow()
                }
            },
            {type: 'separator'},
            {
                label: 'Close Window',
                accelerator: process.platform === 'darwin' ? 'Command+W' : 'Ctrl+W',
                click: () => {
                    if (activeWindow !== null) {
                        activeWindow.close()
                    }
                }
            }
        ]
    })

    Menu.setApplicationMenu(Menu.buildFromTemplate(menuTemplate))

    app.on('browser-window-created', (_, window) => {
        optimizer.watchWindowShortcuts(window)
    })

    return createWindow();
});

app.on('window-all-closed', () => {
    app.quit();
})
