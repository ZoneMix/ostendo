# Theme List

29 built-in themes in `themes/*.yaml`, embedded at compile time via `build.rs`.
All themes pass WCAG 2.0 contrast validation (text:bg >= 4.5:1, accent:bg >= 3.0:1, code_bg:bg >= 1.2:1).

## Dark Themes

| Slug | Name | Background | Accent | Text |
|---|---|---|---|---|
| `terminal_green` | Terminal Green | `#0D0D0D` | `#00FF88` | `#E0E0E0` |
| `amber_warning` | Amber Warning | `#0D0D0D` | `#FFB000` | `#E0E0E0` |
| `arctic_blue` | Arctic Blue | `#0A0E17` | `#00D4FF` | `#E0E0E0` |
| `aurora` | Aurora | `#0C1A14` | `#47D4A0` | `#D0E8DC` |
| `blood_moon` | Blood Moon | `#1a0000` | `#CC0000` | `#FFFFFF` |
| `blueprint` | Blueprint | `#0A1628` | `#5BA4CF` | `#E0E0E0` |
| `catppuccin` | Catppuccin | `#1e1e2e` | `#CBA6F7` | `#CDD6F4` |
| `copper_rose` | Copper Rose | `#1A1214` | `#D4836B` | `#E4D4CC` |
| `cyber_red` | Cyber Red | `#1A1018` | `#FF4444` | `#FFFFFF` |
| `dracula` | Dracula | `#282a36` | `#BD93F9` | `#F8F8F2` |
| `ember` | Ember | `#1A1210` | `#E8944C` | `#E4D8CC` |
| `frost_glass` | Frost Glass | `#0f172a` | `#38BDF8` | `#E2E8F0` |
| `matrix` | Matrix | `#000000` | `#00FF41` | `#C8E6C8` |
| `midnight` | Midnight | `#080C1A` | `#5B8DEF` | `#FFFFFF` |
| `military_green` | Military Green | `#1C2418` | `#4A7C3F` | `#FFFFFF` |
| `neon_purple` | Neon Purple | `#13111C` | `#A855F7` | `#FFFFFF` |
| `nord` | Nord | `#2e3440` | `#88C0D0` | `#D8DEE9` |
| `ocean_deep` | Ocean Deep | `#0A1628` | `#4DB8D4` | `#D4E4EE` |
| `outrun` | Outrun | `#1a0a2e` | `#FF2975` | `#E0D0FF` |
| `solarized` | Solarized | `#002b36` | `#B58900` | `#839496` |
| `sunset_warm` | Sunset Warm | `#1A0A0A` | `#FF6B35` | `#E0E0E0` |
| `twilight` | Twilight | `#14101E` | `#A78BDA` | `#D8D0E8` |
| `vaporwave` | Vaporwave | `#1a0033` | `#FF6EC7` | `#E0E0FF` |
| `sunray` | Sunray | `#45005C`| `#FFD37A` | `#FBEBFF` |

## Light Themes

| Slug | Name | Background | Accent | Text |
|---|---|---|---|---|
| `clean_light` | Clean Light | `#F7F5F0` | `#3B6EC2` | `#1A1A1A` |
| `dracula_light` | Dracula Light | `#FAF9F6` | `#7C5AC7` | `#282A36` |
| `minimal_mono` | Minimal Mono | `#FAFAF8` | `#C4384A` | `#1A1A1A` |
| `nord_light` | Nord Light | `#ECEFF4` | `#5E81AC` | `#2E3440` |
| `paper` | Paper | `#FAF8F5` | `#6B4C3B` | `#2C2C2C` |
| `terminal_green_light` | Terminal Green Light | `#F4F5F0` | `#1A7D42` | `#1A1A1A` |

## Dark/Light Pairs

These themes support `D` key toggle:

| Dark Slug | Light Slug |
|---|---|
| `terminal_green` | `terminal_green_light` |
| `dracula` | `dracula_light` |
| `nord` | `nord_light` |

## Setting Themes

- Front matter: `theme: dracula`
- CLI flag: `--theme dracula`
- Runtime command: `:theme dracula`
- Runtime toggle: `D` key (dark/light variant swap)

## Theme YAML Structure

```yaml
name: "Theme Name"
slug: "theme_slug"
dark_variant: "theme_slug_light"   # optional
colors:
  background: "#282a36"
  text: "#f8f8f2"
  accent: "#bd93f9"
  code_background: "#44475a"
gradient:                           # optional
  from: "#282a36"
  to: "#1a1c24"
  direction: "vertical"
title_decoration: "underline"       # optional
```
