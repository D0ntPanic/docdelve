import {contextBridge, ipcRenderer} from 'electron';
import {ItemPath, SearchParameters, SearchResult} from "../../docdelve_ffi";

contextBridge.exposeInMainWorld('api', {
    search: (path: ItemPath | null, query: string,
             parameters: SearchParameters | null): Promise<Array<SearchResult>> =>
        ipcRenderer.invoke('search', path, query, parameters),
    pageForPath: (identifier: string, url: string, path: ItemPath | null): Promise<OptionalItemPath> =>
        ipcRenderer.invoke('page-for-path', identifier, url, path),
    itemContentsAtPath: (path: ItemPath | ExtendedItemPath): Promise<ExtendedItemContents> =>
        ipcRenderer.invoke('item-contents-at-path', path),
    onWindowActive: (callback: () => void) => ipcRenderer.on('window-active', (_event, _value) => {
        callback()
    }),
    onWindowInactive: (callback: () => void) => ipcRenderer.on('window-inactive', (_event, _value) => {
        callback()
    }),
    onFocusSearch: (callback: () => void) => ipcRenderer.on('focus-search', (_event, _value) => {
        callback()
    }),
    onThemeUpdated: (callback: () => void) => ipcRenderer.on('theme-updated', (_event, _value) => {
        callback()
    }),
    onNavigateBack: (callback: () => void) => ipcRenderer.on('navigate-back', (_event, _value) => {
        callback()
    }),
    onNavigateForward: (callback: () => void) => ipcRenderer.on('navigate-forward', (_event, _value) => {
        callback()
    })
});

contextBridge.exposeInMainWorld('platform', process.platform)
