import {useState, useRef, useEffect} from 'react'
import {WebviewTag, PageTitleUpdatedEvent, DidNavigateEvent} from 'electron'
import TitleBar from './titlebar'
import Sidebar from './sidebar'
import {Split} from '@geoffcox/react-splitter'

let navigateBackCallback = () => {
}
let navigateForwardCallback = () => {
}
let themeUpdatedCallback = () => {
}
window.api.onNavigateBack(() => {
    navigateBackCallback()
})
window.api.onNavigateForward(() => {
    navigateForwardCallback()
})
window.api.onThemeUpdated(() => {
    themeUpdatedCallback()
})

export default function Root() {
    const [chestTag, setChestTag] = useState<string | null>(null)
    const [pagePath, setPagePath] = useState<ExtendedItemPath | null>(null)
    const [itemPath, setItemPath] = useState<ExtendedItemPath | null>(null)
    const [title, setTitle] = useState<string>("")
    const content = useRef<HTMLWebViewElement>(null)

    function onNavigate(path: ExtendedItemPath, url: string) {
        if (content.current !== null) {
            content.current.classList.remove('empty')
            const fullURL = new URL(url, "docs://" + path.identifier + "/");

            (content.current as WebviewTag).loadURL(fullURL.toString()).then(() => null)
        }

        setItemPath(path)
    }

    useEffect(() => {
        navigateBackCallback = () => {
            if (content.current !== null) {
                (content.current as WebviewTag).goBack()
            }
        }

        navigateForwardCallback = () => {
            if (content.current !== null) {
                (content.current as WebviewTag).goForward()
            }
        }

        themeUpdatedCallback = () => {
            if (content.current !== null) {
                (content.current as WebviewTag).reload()
            }
        }

        content.current?.addEventListener('did-stop-loading', () => {
            if (content.current !== null) {
                content.current.classList.remove('hidden')
                if (!content.current.classList.contains('empty')) {
                    content.current.focus()
                }
            }
        })

        content.current?.addEventListener('page-title-updated', (event: Event) => {
            const titleEvent = event as PageTitleUpdatedEvent
            setTitle(titleEvent.title)
            document.title = "Doc Delve - " + titleEvent.title
        })

        content.current?.addEventListener('did-navigate', (event: Event) => {
            const navigateEvent = event as DidNavigateEvent
            const url = new URL(navigateEvent.url)
            if (url.protocol === "docs:") {
                // Documentation chest URL, break into the identifier and chest path
                let fullPath = url.pathname
                if (fullPath.startsWith("//")) {
                    fullPath = fullPath.substring(2)
                    const identifierPartIdx = fullPath.indexOf("/")
                    if (identifierPartIdx !== -1) {
                        const identifier = fullPath.substring(0, identifierPartIdx)
                        const path = fullPath.substring(identifierPartIdx + 1)

                        // Look up the chest path for the page at this URL
                        window.api.pageForPath(identifier, path, itemPath).then((page: OptionalItemPath) => {
                            setChestTag(page.chestTag)
                            if (page.chestPath === null) {
                                setPagePath(null)
                                setItemPath(null)
                            } else {
                                setPagePath({
                                    identifier: page.identifier,
                                    chestTag: page.chestTag,
                                    chestPath: page.chestPath
                                })
                            }
                        })
                        return;
                    }
                }
            } else if (url.protocol === "file:") {
                if (url.pathname.endsWith("/blank.html")) {
                    setChestTag("Home")
                    setPagePath(null)
                    setItemPath(null)
                    return;
                }
            }

            setChestTag(null)
            setPagePath(null)
            setItemPath(null)
        })
    }, [])

    return <>
        <TitleBar chestTag={chestTag} pagePath={pagePath} pageTitle={title} onNavigate={onNavigate}/>
        <div id="windowContent">
            <Split initialPrimarySize="200px" minPrimarySize="150px" minSecondarySize="250px" splitterSize="4px">
                <Sidebar pagePath={pagePath} onNavigate={onNavigate}/>
                <div id="contentContainer">
                    <webview key="content" className="content hidden empty" src="./blank.html" ref={content}/>
                </div>
            </Split>
        </div>
    </>;
}
