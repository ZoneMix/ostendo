# Presentation Assets

Place images and other media files referenced by example presentations in this directory.

## Supported formats

- **Images**: PNG, JPEG, SVG, GIF
- **Diagrams**: Mermaid source files (rendered via `mmdc` CLI)

## Usage in presentations

Reference assets with relative paths from your markdown file:

```markdown
![Description](assets/my_image.png)
<!-- image_scale: 50 -->
```

For ASCII art rendering (works in any terminal):

```markdown
![Description](assets/my_image.png)
<!-- image_render: ascii -->
```

## Image protocols

Ostendo auto-detects the best rendering protocol for your terminal:

| Protocol | Terminal | Quality |
|:---------|:---------|:--------|
| Kitty    | Kitty    | Best    |
| iTerm2   | iTerm2   | Good    |
| Sixel    | Various  | Good    |
| ASCII    | Any      | Basic   |
