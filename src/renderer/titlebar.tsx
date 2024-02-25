import React, {useState, useRef, useEffect, ReactElement} from 'react'
import SearchResults, {SearchResultItem, processSearchResults} from './searchresults'
import {ChestPathElement, ChestItemType} from '../../docdelve_ffi'

export interface CachedSearchResult {
    query: string;
    items: Array<SearchResultItem>;
}

let windowActiveCallback = () => {
}
let windowInactiveCallback = () => {
}
let focusSearchCallback = () => {
}
window.api.onWindowActive(() => {
    windowActiveCallback()
})
window.api.onWindowInactive(() => {
    windowInactiveCallback()
})
window.api.onFocusSearch(() => {
    focusSearchCallback()
})

export function TitleBarCurrentPath({tag, elements}: { tag: string, elements: Array<ChestPathElement> }) {
    let renderedElements = elements.map((element: ChestPathElement) =>
        <>
            <span className="currentChestPathElementSeparator"> ≫ </span>
            <span className="currentChestPathElement">{element.name}</span>
        </>)

    return <div id="titleBarCurrentPath">
        <span className="currentChestTag">{tag}</span>
        {renderedElements}
    </div>
}

export default function TitleBar({chestTag, pagePath, pageTitle, onNavigate}: {
    chestTag: string | null,
    pagePath: ExtendedItemPath | null,
    pageTitle: string,
    onNavigate: (path: ExtendedItemPath, url: string) => void
}) {
    const [searchQuery, setSearchQuery] = useState<string>('')
    const [searchResults, setSearchResults] =
        useState<CachedSearchResult>({query: "", items: []})
    const titleBarRef = useRef<HTMLDivElement>(null);
    const searchBoxRef = useRef<HTMLInputElement>(null);
    const searchAreaRef = useRef<HTMLDivElement>(null);
    const searchResultRef = useRef<HTMLDivElement>(null);

    if (searchQuery != searchResults.query) {
        window.api.search(null, searchQuery, {resultCount: 50})
            .then((result: Array<ExtendedSearchResult>) => {
                setSearchResults({query: searchQuery, items: processSearchResults(result)})
            })
    }

    function navigateToResult(item: SearchResultItem) {
        if (item.item.url !== undefined) {
            onNavigate({
                identifier: item.result.result.path.identifier,
                chestTag: item.result.chestTag,
                chestPath: item.result.result.path.chestPath
            }, item.item.url)
            setSearchQuery("")
        }
    }

    function searchQueryChanged(event: React.ChangeEvent<HTMLInputElement>) {
        setSearchQuery(event.target.value)
    }

    function searchKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
        if (event.key === 'Enter' && searchQuery == searchResults.query && searchResults.items.length > 0) {
            navigateToResult(searchResults.items[0])
        } else if (event.key === 'Escape') {
            setSearchQuery("")
        } else if (event.key === 'ArrowDown') {
            searchResultRef.current?.focus()
            event.preventDefault()
            event.stopPropagation()
        }
    }

    function handleLostFocus(event: React.FocusEvent<HTMLDivElement>) {
        // See if the focus is moving somewhere within the search area, like the results list.
        // If so, don't clear the search results.
        let focus = event.relatedTarget;
        while (focus !== null && focus instanceof Element) {
            if (focus === searchAreaRef.current) {
                return;
            }
            focus = focus.parentElement;
        }

        // If focus is moving somewhere else, clear the search results.
        setSearchQuery("")
    }

    function cancelSearch() {
        setSearchQuery("")
        searchBoxRef.current?.focus()
    }

    useEffect(() => {
        windowActiveCallback = () => titleBarRef.current?.classList.remove('inactive')
        windowInactiveCallback = () => titleBarRef.current?.classList.add('inactive')
        focusSearchCallback = () => searchBoxRef.current?.focus()
    }, [])

    let currentPathElement: ReactElement;
    if (pagePath === null) {
        if (chestTag === null) {
            if (pageTitle.length === 0) {
                currentPathElement = <></>
            } else {
                currentPathElement =
                    <TitleBarCurrentPath
                        tag="External"
                        elements={[{elementType: "Page" as ChestItemType, name: pageTitle}]}/>
            }
        } else {
            currentPathElement =
                <TitleBarCurrentPath
                    tag={chestTag}
                    elements={[{elementType: "Page" as ChestItemType, name: pageTitle}]}/>
        }
    } else {
        currentPathElement = <TitleBarCurrentPath tag={pagePath.chestTag} elements={pagePath.chestPath.elements}/>
    }

    return <div id="titleBar" className="draggable" ref={titleBarRef}>
        <div id="titleBarContent">
            <span id="windowTitle">
                <div className="windowTitleText">Doc Delve</div>
                {currentPathElement}
            </span>
            <div id="searchArea" className="nonDraggable" ref={searchAreaRef}>
                <input id="searchBox" type="text" placeholder="Search" autoCapitalize="off"
                       autoComplete="off" autoCorrect="off" spellCheck="false" value={searchQuery}
                       onChange={searchQueryChanged} onKeyDown={searchKeyDown} onBlur={handleLostFocus}
                       ref={searchBoxRef}/>
                <SearchResults search={searchQuery} items={searchResults.items} focusTarget={searchResultRef}
                               onFocusLost={handleLostFocus} onNavigate={navigateToResult}
                               onCancelSearch={cancelSearch}/>
            </div>
        </div>
    </div>
}
