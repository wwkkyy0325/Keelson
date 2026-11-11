# Assets

Only files the user supplied, or files whose license the user confirmed.
The rig does not download images or footage from the web.

Scenes load a file here as `/assets/<name>`; a scene may draw it with
`new Image()` onto the canvas. Record every file's origin and license in
`assets.md`:

```
| file | source | license |
|---|---|---|
| photo.jpg | user-supplied, 2026-11-11 | CC BY 4.0 |
```
