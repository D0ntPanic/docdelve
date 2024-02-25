interface Window {
    api: {
        search: (path: ItemPath | null, query: string,
                 parameters: SearchParameters | null) => Promise<Array<ExtendedSearchResult>>,
        pageForPath: (identifier: string, url: string, path: ItemPath | null) => Promise<OptionalItemPath>,
        itemContentsAtPath: (path: ItemPath | ExtendedItemPath) => Promise<ExtendedItemContents>,
        onWindowActive: (callback: () => void) => void,
        onWindowInactive: (callback: () => void) => void,
        onFocusSearch: (callback: () => void) => void,
        onThemeUpdated: (callback: () => void) => void,
        onNavigateBack: (callback: () => void) => void,
        onNavigateForward: (callback: () => void) => void
    },
    platform: string
}

interface ExtendedSearchResult {
    result: SearchResult,
    chestTag: string,
    items: Array<ChestItem>,
}

interface ExtendedItemPath {
    identifier: string,
    chestTag: string,
    chestPath: ChestPath,
}

interface OptionalItemPath {
    identifier: string,
    chestTag: string,
    chestPath: ChestPath | null,
}

interface ExtendedItemContents {
    chestItems: Array<ChestItem>,
    pageItems: Array<PageItem>,
    bases: Array<BaseItems>,
}

interface BaseItems {
    path: ItemPath,
    items: Array<ChestItem>,
}
