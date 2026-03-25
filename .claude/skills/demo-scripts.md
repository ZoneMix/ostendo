# Demo Scripts

Use when creating live code demos in Ostendo presentations.

## Modes

- `+exec`: run code, capture stdout/stderr, display inline
- `+pty`: run in pseudo-terminal, preserve ANSI codes

## Fence Syntax

Open a code block with language, mode, and optional label on the fence line.
Languages: Python, Bash, JavaScript, Ruby, Rust, C, C++, Go.
Compiled languages (Rust, C, C++, Go) auto-wrap snippets without a main function.

## Preambles

Shared setup prepended to exec blocks of a language on that slide:

```
<!-- preamble_start: python -->
import requests
<!-- preamble_end -->
```

## Limits

- Code: 64 KB max
- Output: 1 MB max
- Timeout: 30 seconds
- Sandbox: setsid process groups
- --no-exec disables execution
- --remote-exec gates WebSocket execution

## Flow

1. Ctrl+E triggers execution
2. Background thread streams output via mpsc
3. poll_exec_output() picks up lines
4. Output rendered inline below code block
5. ANSI codes preserved

## Tips

- Use +exec for safe, fast code only
- Add labels for audience clarity
- Use +pty when output has colors
- Keep under 5 seconds
- Use preambles for shared imports
