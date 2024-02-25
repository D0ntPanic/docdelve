import {useState, useRef, ReactElement} from 'react'
import {ChestItem, PageItem} from '../../docdelve_ffi'

interface CachedPathItems {
    path: ExtendedItemPath | null;
    items: ExtendedItemContents;
}

interface ItemType {
    name: string;
    heading: string;
}

const itemTypes: Array<ItemType> = [
    {name: "Page", heading: "Documentation"},
    {name: "Namespace", heading: "Namespaces"},
    {name: "Module", heading: "Modules"},
    {name: "Group", heading: "Groups"},
    {name: "Class", heading: "Classes"},
    {name: "Struct", heading: "Structures"},
    {name: "Union", heading: "Unions"},
    {name: "Object", heading: "Objects"},
    {name: "Trait", heading: "Traits"},
    {name: "Function", heading: "Functions"},
    {name: "Method", heading: "Methods"},
    {name: "Variable", heading: "Variables"},
    {name: "Member", heading: "Members"},
    {name: "Field", heading: "Fields"},
    {name: "Enum", heading: "Enumerations"},
    {name: "Value", heading: "Values"},
    {name: "Variant", heading: "Variants"},
    {name: "Interface", heading: "Interfaces"},
    {name: "TraitImplementation", heading: "Trait Implementations"},
    {name: "Typedef", heading: "Types"},
    {name: "Constant", heading: "Constants"}
]

export function SidebarHeader({name}: { name: string }) {
    return <div className="sidebarHeaderContainer">
        <h2 className="sidebarHeader">{name}</h2>
    </div>
}

export function SidebarSectionEnd() {
    return <div className="sidebarSectionEnd"/>
}

export function SidebarChestItem({page, item, onNavigate}: {
    page: ExtendedItemPath,
    item: ChestItem,
    onNavigate: (path: ExtendedItemPath, url: string) => void
}) {
    function handleClick() {
        if (item.url !== undefined) {
            let elements = Array(page.chestPath.elements)
            elements.push({elementType: item.itemType, name: item.name})

            onNavigate({
                identifier: page.identifier,
                chestTag: page.chestTag,
                chestPath: {elements: elements}
            }, item.url)
        }
    }

    return <div className="sidebarItem">
        <div className="sidebarItemName" onClick={handleClick}>{item.name}</div>
    </div>
}

export function SidebarPageItem({page, item, depth, onNavigate}: {
    page: ExtendedItemPath,
    item: PageItem,
    depth: number,
    onNavigate: (path: ExtendedItemPath, url: string) => void
}) {
    function handleClick() {
        if (item.url !== undefined) {
            onNavigate(page, item.url)
        }
    }

    let padding = depth * 10 + 20
    return <div className="sidebarItem">
        <div className="sidebarItemName" style={{paddingLeft: padding.toString() + "px", textIndent: "-20px"}}
             onClick={handleClick}>
            {item.title}
        </div>
    </div>
}

export function SidebarEmpty() {
    return <div className="sidebarEmpty">This page does not have any indexed content.</div>
}

export default function Sidebar({pagePath, onNavigate}: {
    pagePath: ExtendedItemPath | null,
    onNavigate: (path: ExtendedItemPath, url: string) => void
}) {
    const [items, setItems] =
        useState<CachedPathItems>({path: null, items: {chestItems: [], pageItems: [], bases: []}})
    const contentRef = useRef<HTMLDivElement>(null)

    if (pagePath === null) {
        return <div id="sidebarContainer" className="empty" ref={contentRef}>
            <div id="sidebarContent">
                <SidebarEmpty/>
            </div>
        </div>
    }

    if (pagePath != items.path) {
        if (pagePath !== null) {
            window.api.itemContentsAtPath(pagePath)
                .then((result: ExtendedItemContents) => {
                    setItems({path: pagePath, items: result})
                    contentRef.current?.scroll(0, 0)
                })
        }
    }

    let itemsByType: Map<string, Array<ChestItem>> = new Map();
    items.items.chestItems.forEach((item: ChestItem) => {
        if (item.itemType === "Module" || item.itemType === "Group" || item.itemType === "Page") {
            if (itemsByType.has(item.itemType)) {
                let items = itemsByType.get(item.itemType)!;
                if (items[items.length - 1].name !== item.name) {
                    items.push(item)
                }
            } else {
                itemsByType.set(item.itemType, [item])
            }
        } else if (item.itemType === "Object") {
            if (itemsByType.has(item.objectType!)) {
                let items = itemsByType.get(item.objectType!)!;
                if (items[items.length - 1].name !== item.name) {
                    items.push(item)
                }
            } else {
                itemsByType.set(item.objectType!, [item])
            }
        }
    })

    let elements: Array<ReactElement> = []

    if (items.items.pageItems.length > 0) {
        elements.push(<SidebarHeader name="Contents"/>)

        function processPageItems(items: Array<PageItem>, depth: number) {
            items.forEach((item: PageItem) => {
                elements.push(<SidebarPageItem page={pagePath!} item={item} depth={depth} onNavigate={onNavigate}/>)
                if (item.itemType === "Category") {
                    processPageItems(item.contents, depth + 1)
                }
            })
        }

        processPageItems(items.items.pageItems, 0)
        elements.push(<SidebarSectionEnd/>)
    }

    if (items.items.bases.length > 0) {
        elements.push(<SidebarHeader name="Base"/>)
        items.items.bases.forEach((item: BaseItems) => {
            if (item.items.length > 0) {
                elements.push(<SidebarChestItem page={item.path} item={item.items[0]} onNavigate={onNavigate}/>)
            }
        })
        elements.push(<SidebarSectionEnd/>)
    }

    itemTypes.forEach((itemType: ItemType) => {
        if (itemsByType.has(itemType.name)) {
            elements.push(<SidebarHeader name={itemType.heading}/>)
            itemsByType.get(itemType.name)!.forEach((item: ChestItem) => {
                elements.push(<SidebarChestItem page={pagePath} item={item} onNavigate={onNavigate}/>)
            })
            elements.push(<SidebarSectionEnd/>)
        }
    })

    if (elements.length === 0) {
        return <div id="sidebarContainer" className="empty" ref={contentRef}>
            <div id="sidebarContent">
                <SidebarEmpty/>
            </div>
        </div>
    }

    return <div id="sidebarContainer" ref={contentRef}>
        <div id="sidebarContent">
            {elements}
        </div>
    </div>
}
