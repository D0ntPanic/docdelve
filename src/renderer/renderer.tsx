import React from 'react'
import {createRoot} from 'react-dom/client'
import Root from "./root"
import './style.css'

const content = document.getElementById('root')
if (content === null) {
    throw new Error('Could not find root element')
}

let platform = "other";
switch (window.platform) {
    case "darwin":
        platform = "macos";
        break;
    case "win32":
        platform = "windows";
        break;
}
document.body.classList.add(platform);

const root = createRoot(content);
root.render(
    <React.StrictMode>
        <Root/>
    </React.StrictMode>
)
