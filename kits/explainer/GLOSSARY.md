# Glossary — English to Chinese

The kit's terms and their Chinese names. Command names, paths and identifiers
stay in English.

## The film

| English | 中文 | Note |
|---------|------|------|
| explainer (rig) | 解说片装置 | writes and renders an explainer |
| rig | 装置 | the file that configures the agent |
| script | 文稿 | the written narration |
| storyboard | 分镜稿 | scenes, narration, visual, `asset_type` |
| scene | 镜头 / 场景 | one timed shot |
| shot | 镜头 | a camera take |
| narration | 旁白 | the voice text |
| voice | 配音 | one voice per film |
| subtitle cue | 字幕条 | one timed line pair |
| subtitle band | 字幕带 | the bottom safe band |
| measured duration | 实测时长 | read from the audio, never typed |
| safe rectangle | 安全区 | where a 3D subject must fit |
| contact sheet | 联系表 | the QA thumbnails |
| register | 表达语域 | the 2D or the 3D mode |
| mixed film | 混排片 | 2D and 3D in one film |
| descriptive form | 描述形式 | 2D for the model, 3D for space |

## Claims and evidence

| English | 中文 | Note |
|---------|------|------|
| claim | 论断 | fact, interpretation or value |
| claim ledger | 论断台账 | `claims.json` |
| fact | 事实 | needs two domains or a primary source |
| interpretation | 解释 | names its framework |
| value judgment | 价值判断 | labelled on screen |
| source | 来源 | publisher, title, URL, access date |
| source card | 来源卡 | the publisher named on screen |
| glossary (on screen) | 术语卡 | one term per concept |
| 示意 | schematic | a drawing, not a measurement |
| conflict | 冲突 | both sides stay in the open |

## Rendering

| English | 中文 | Note |
|---------|------|------|
| render | 渲染 | frames from a page |
| frame | 帧 | one PNG |
| frame sink | 帧接收器 | the loopback receiver |
| page | 渲染页面 | `kit/scene.html` or `kit/scene3d.html` |
| harness | 渲染外壳 | the page that drives a scene |
| asset_type | 场景类型 | `canvas` or `scene3d` |
| canvas | 纸面场景 | the flat design kit |
| scene3d | 3D 场景 | the WebGL page |
| camera fit | 镜头适配 | the subject stays inside the safe rectangle |
| bloom | 辉光 | the 2D pass over the 3D layer |
| trail | 拖尾 | a parcel's short streak |
| seed | 种子 | makes a frame repeatable |
| view_image | 看帧 | read a frame as an image |
| render_scene | 渲染单景 | one scene, while iterating |
| render_film | 渲染整片 | every scene, from the storyboard |
| assemble | 合成 | frames plus voice into the film |
| loudness | 响度 | LUFS, target about −16 |
| probe | 探测 | read a finished file's streams |

## The 3D room

| English | 中文 | Note |
|---------|------|------|
| CFD | 计算流体力学 | the film is not a CFD result |
| stream function | 流函数 | makes the flow tangent to the walls |
| field | 流场 | analytic and seeded |
| intake | 进风口 | the return |
| vent | 出风口 | the supply |
| particle / parcel | 粒子 / 气团 | a parcel of air |
| buoyancy | 浮力 | warm rises, cold sinks |
| vapor-compression loop | 蒸气压缩循环 | the refrigeration cycle |
| evaporator | 蒸发器 | absorbs heat indoors |
| compressor | 压缩机 | adds work |
| condenser | 冷凝器 | rejects heat outdoors |
| expansion valve | 膨胀阀 | drops the pressure |
| refrigerant | 制冷剂 | changes state around the loop |
| Q_in / Q_out / W | 吸热量 / 排热量 / 压缩机功 | `Q_out = Q_in + W` |

## Quality gates

| English | 中文 | Note |
|---------|------|------|
| gate | 门槛 | a check that can fail the film |
| blank frame | 空帧 | a frame with no content |
| data check | 数据校验 | recompute the numbers |
| claims check | 论断校验 | source and scope |
| loudness check | 响度校验 | integrated LUFS |
