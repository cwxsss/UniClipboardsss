# CLAUDE.md

This file is a compatibility entrypoint for Claude Code.

Single source of truth: [`AGENTS.md`](./AGENTS.md).

Load `AGENTS.md` first. It is the root navigation index and tells the agent which focused documents to read on demand.

Do not maintain separate project memory here.

重要指令（Zed 编辑器专用）：
当你需要引用任何文件、目录、函数或具体行号时，**严格遵守以下格式，不要使用任何 Markdown 链接** [text](path)：
1. 文件名/路径用反引号包裹：`src/services/.../io_handlers.py`
2. 带行号时用：`io_handlers.py:134` 或 `io_handlers.py#L134`
3. 显示代码时，必须用**路径开头的代码块**（不要加语言标识符如 ```python）：
   ```/home/wuy6/projects/.../io_handlers.py#L134-150
   你的代码内容