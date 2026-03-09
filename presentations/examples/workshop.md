---
title: Interactive Workshop
author: Workshop Lead
date: 2026-03-09
accent: "#FFD700"
transition: slide
---

# Interactive Workshop
<!-- section: intro -->
<!-- font_size: 6 -->
<!-- ascii_title -->

Hands-on coding exercises in the terminal

- Follow along with live code execution
- Press **Ctrl+E** to run code blocks
- Press `n` to view speaker notes for hints

<!-- notes: Welcome participants and ensure everyone has ostendo installed. Remind them about Ctrl+E for code execution. -->

---

# Agenda
<!-- section: intro -->
<!-- font_size: 6 -->

Today's workshop covers:

- **Python Fundamentals** -- data structures and comprehensions
- **Bash Scripting** -- system commands and pipelines
- **Exercises** -- hands-on practice problems

| Section | Duration | Type |
|:--------|:--------:|:-----|
| Python  | 20 min   | Demo + hands-on |
| Bash    | 15 min   | Demo + hands-on |
| Exercises | 25 min | Self-paced |

> All code blocks are executable -- press **Ctrl+E** to run them live

<!-- notes: Adjust timing based on audience experience level. The exercises section can be extended if needed. -->

---

# Python: Data Structures
<!-- section: python -->
<!-- font_size: 6 -->

```python +exec {label: "data_structures.py"}
# Lists and comprehensions
numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
evens = [n for n in numbers if n % 2 == 0]
squares = {n: n**2 for n in range(1, 6)}

print(f"Numbers: {numbers}")
print(f"Evens:   {evens}")
print(f"Squares: {squares}")

# String formatting
for name, score in [("Alice", 95), ("Bob", 87), ("Charlie", 92)]:
    grade = "A" if score >= 90 else "B"
    print(f"  {name:>10}: {score} ({grade})")
```

- List comprehensions filter and transform data
- Dict comprehensions build mappings inline
- f-strings provide readable formatting

<!-- notes: Walk through each data structure. Ask participants to modify the comprehension filter before running. -->

---

# Bash: System Commands
<!-- section: bash -->
<!-- font_size: 6 -->

```bash +exec {label: "system_info.sh"}
echo "=== System Info ==="
echo "Host:     $(hostname)"
echo "User:     $(whoami)"
echo "Shell:    $SHELL"
echo "Date:     $(date '+%Y-%m-%d %H:%M:%S')"
echo ""
echo "=== Disk Usage (top 3) ==="
df -h 2>/dev/null || echo "(df not available)"
echo ""
echo "=== Environment ==="
echo "PATH entries: $(echo $PATH | tr ':' '\n' | wc -l | tr -d ' ')"
echo "Env vars:     $(env | wc -l | tr -d ' ')"
```

- Command substitution: `$(command)`
- Pipes chain commands: `cmd1 | cmd2`
- Fallback with `||` for portability

<!-- notes: Demonstrate how bash pipelines work. Encourage participants to add their own commands. -->

---

# Exercises & Next Steps
<!-- section: exercises -->
<!-- font_size: 6 -->
<!-- title_decoration: underline -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Try These Exercises**
- Modify the Python code to filter odd numbers
- Add a `top` or `ps` command to the bash block
- Create your own `+exec` code block
- Try `:theme dracula` for a different look

<!-- column: 1 -->

**Resources**
- Press `?` for keyboard shortcuts
- Use `:overview` to see all slides
- Hot reload: edit this file while presenting
- Export: `ostendo --export html workshop.md`

<!-- reset_layout -->

> Ostendo makes workshops interactive -- every code block is a live playground.

<!-- notes:
Give participants 15-25 minutes for self-paced exercises.
Circulate and help with questions.
Wrap up by showing how to create their own presentations.
-->
