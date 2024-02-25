import React, {useRef, RefObject, ReactElement} from 'react'
import {ChestItem, ChestPathElement} from "../../docdelve_ffi"

export interface SearchResultItem {
    result: ExtendedSearchResult;
    item: ChestItem;
    renderStyle: RenderStyle;
    oddRow: boolean;
    lastItem: boolean;
}

export enum RenderStyle {
    NameOnly,
    NameAndDeclaration,
    AdditionalDeclaration
}

export function processSearchResults(results: Array<ExtendedSearchResult>): Array<SearchResultItem> {
    let resultItems: Array<SearchResultItem> = [];
    let row = 0;
    results.forEach((result: ExtendedSearchResult) => {
        let items = result.items.filter((item: ChestItem) => item.url !== undefined)
        if (items.length == 0)
            return;

        let declarations: Array<ChestItem> = [];
        items.forEach((item: ChestItem) => {
            if (item.declaration !== undefined && item.declaration !== item.name) {
                declarations.push(item);
            }
        })

        if (declarations.length == 0) {
            resultItems.push({
                result: result,
                item: items[0],
                renderStyle: RenderStyle.NameOnly,
                oddRow: row % 2 != 0,
                lastItem: true
            })
        } else if (declarations.length == 1) {
            resultItems.push({
                result: result,
                item: declarations[0],
                renderStyle: RenderStyle.NameAndDeclaration,
                oddRow: row % 2 != 0,
                lastItem: true
            })
        } else {
            resultItems.push({
                result: result,
                item: declarations[0],
                renderStyle: RenderStyle.NameOnly,
                oddRow: row % 2 != 0,
                lastItem: false
            })
            declarations.forEach((declaration: ChestItem, i) => {
                resultItems.push({
                    result: result,
                    item: declaration,
                    renderStyle: RenderStyle.AdditionalDeclaration,
                    oddRow: row % 2 != 0,
                    lastItem: i === declarations.length - 1
                })
            })
        }

        row++;
    })
    return resultItems
}

export function ResultPath({item}: { item: SearchResultItem }) {
    let elements = item.result.result.path.chestPath.elements.slice(0, -1).map((element: ChestPathElement) =>
        <>
            <span className="chestPathElementSeparator"> ≫ </span>
            <span className="chestPathElement">{element.name}</span>
        </>)

    return <div className="searchResultPath">
        <span className="chestTag">{item.result.chestTag}</span>
        {elements}
    </div>
}

export function SingleResult({item}: { item: SearchResultItem }) {
    let name = item.item.name;
    if (item.item.fullName !== undefined) {
        name = item.item.fullName;
    }

    switch (item.renderStyle) {
        case RenderStyle.NameAndDeclaration:
            return <>
                <ResultPath item={item}/>
                <div className="searchResultName">{name}</div>
                <div className="searchResultDeclaration">{item.item.declaration}</div>
            </>
        case RenderStyle.AdditionalDeclaration:
            return <div className="searchResultDeclaration">{item.item.declaration}</div>
        default:
            return <>
                <ResultPath item={item}/>
                <div className="searchResultName">{name}</div>
            </>
    }
}

export default function SearchResults({search, items, focusTarget, onFocusLost, onNavigate, onCancelSearch}: {
    search: string,
    items: Array<SearchResultItem>,
    focusTarget: RefObject<HTMLDivElement>,
    onFocusLost: (event: React.FocusEvent<HTMLDivElement>) => void,
    onNavigate: (result: SearchResultItem) => void,
    onCancelSearch: () => void
}) {
    if (search.length === 0) {
        return <></>;
    } else {
        let refs: RefObject<Map<number, HTMLDivElement>> = useRef(new Map());

        let noResultsMessage: ReactElement;
        if (items.length == 0) {
            noResultsMessage = <div className="searchResult faded">No matching results.</div>;
        } else {
            noResultsMessage = <></>;
        }

        function itemCount() {
            if (refs.current === null) {
                return 0;
            }
            let result = 0;
            while (refs.current.has(result)) {
                result++;
            }
            return result;
        }

        function currentItem() {
            let current: number | null = null;
            if (refs.current !== null) {
                for (let i = 0; refs.current.has(i); i++) {
                    if (refs.current.get(i)!.classList.contains("current")) {
                        current = i;
                        break;
                    }
                }
            }
            return current;
        }

        function handleFocus(_event: React.FocusEvent<HTMLDivElement>) {
            if (refs.current === null)
                return;
            let current = currentItem();
            if (current === null && refs.current.has(0)) {
                refs.current.get(0)!.classList.add("current");
                refs.current.get(0)!.scrollIntoView({behavior: "instant", block: "nearest"})
            }
        }

        function handleMouseDown(event: React.MouseEvent<HTMLDivElement>) {
            if (refs.current === null)
                return;
            let current = currentItem();
            if (current !== null)
                refs.current.get(current)!.classList.remove("current");
            event.currentTarget.classList.add("current");
        }

        function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
            if (refs.current === null)
                return;
            let current = currentItem()

            if (event.key === 'ArrowDown') {
                if (current === null) {
                    if (refs.current.has(0)) {
                        refs.current.get(0)!.classList.add("current")
                    }
                } else if (refs.current.has(current + 1)) {
                    refs.current.get(current)!.classList.remove("current")
                    refs.current.get(current + 1)!.classList.add("current")
                    refs.current.get(current + 1)!.scrollIntoView({behavior: "instant", block: "nearest"})
                } else {
                    refs.current.get(current)!.classList.remove("current")
                    refs.current.get(0)!.classList.add("current")
                    refs.current.get(0)!.scrollIntoView({behavior: "instant", block: "nearest"})
                }
                event.preventDefault()
                event.stopPropagation()
            } else if (event.key === 'ArrowUp') {
                if (current === null) {
                    let length = itemCount()
                    if (length > 0) {
                        refs.current.get(length - 1)!.classList.add("current")
                    }
                } else if (current > 0) {
                    refs.current.get(current)!.classList.remove("current")
                    refs.current.get(current - 1)!.classList.add("current")
                    refs.current.get(current - 1)!.scrollIntoView({behavior: "instant", block: "nearest"})
                } else {
                    let length = itemCount()
                    refs.current.get(current)!.classList.remove("current")
                    refs.current.get(length - 1)!.classList.add("current")
                    refs.current.get(length - 1)!.scrollIntoView({behavior: "instant", block: "nearest"})
                }
                event.preventDefault()
                event.stopPropagation()
            } else if (event.key === 'Enter') {
                if (current !== null && refs.current.has(current)) {
                    refs.current.get(current)!.click();
                }
                event.preventDefault()
                event.stopPropagation()
            } else if (event.key === 'Escape') {
                onCancelSearch()
                event.preventDefault()
                event.stopPropagation()
            }
        }

        return <div id="searchResults" key={search}>
            <div id="searchResultsList" tabIndex={0} ref={focusTarget} onFocus={handleFocus} onKeyDown={handleKeyDown}
                 onBlur={onFocusLost}>
                {items.map((item, i) => {
                    let classes = ["searchResult"];
                    if (item.oddRow) {
                        classes.push("oddRow");
                    }
                    if (item.lastItem) {
                        classes.push("lastItem");
                    }
                    return <div className={classes.join(" ")}
                                ref={(element) => {
                                    if (refs.current !== null) {
                                        if (element === null) {
                                            refs.current.delete(i);
                                        } else {
                                            refs.current.set(i, element)
                                        }
                                    }
                                }} onClick={() => onNavigate(item)} onMouseDown={handleMouseDown}>
                        <SingleResult item={item}/>
                    </div>
                })}
                {noResultsMessage}
            </div>
        </div>
    }
}
