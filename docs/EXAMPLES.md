# Ostendo Examples Cookbook

Practical examples for common presentation types.

## 1. Simple Talk

A basic conference talk with sections and speaker notes.

````markdown
---
title: Intro to Rust
theme: frost_glass
---

# Intro to Rust
<!-- section: intro -->
<!-- timing: 2.0 -->
<!-- ascii_title -->

A systems programming language for everyone

<!-- notes: Welcome the audience, mention this is beginner-friendly -->

---

# Why Rust?
<!-- timing: 3.0 -->

- Memory safety **without garbage collection**
- Zero-cost abstractions
- Fearless concurrency
- Great tooling: `cargo`, `clippy`, `rustfmt`

<!-- notes: Emphasize that safety doesn't mean slow -->

---

# Hello World
<!-- section: basics -->
<!-- timing: 1.5 -->

```rust +exec {label: "hello.rs"}
fn main() {
    println!("Hello, world!");
}
```

---

# Ownership
<!-- timing: 3.0 -->

```rust {label: "ownership example"}
fn main() {
    let s1 = String::from("hello");
    let s2 = s1; // s1 is moved
    // println!("{}", s1); // ERROR: s1 no longer valid
    println!("{}", s2);
}
```

> Each value in Rust has exactly one owner

<!-- notes: This is the key concept - draw the ownership diagram on the board -->

---

# Thank You
<!-- section: closing -->

- Docs: *doc.rust-lang.org*
- Community: **friendly and welcoming**

<!-- notes: Plug the Rust community Discord -->
````

## 2. Technical Demo

Heavy on code execution and live demos.

````markdown
---
title: API Testing Demo
theme: terminal_green
---

# API Testing
<!-- section: setup -->
<!-- timing: 1.0 -->
<!-- ascii_title -->

Live demo with Python requests

---

# GET Request
<!-- section: demo -->
<!-- timing: 2.0 -->

```python +exec {label: "basic GET"}
import json
# Simulated API response
response = {"status": 200, "users": ["alice", "bob"]}
print(json.dumps(response, indent=2))
```

<!-- notes: Show the response structure -->

---

# POST Request
<!-- timing: 2.0 -->

```python +exec {label: "POST with payload"}
import json
payload = {"name": "Charlie", "role": "admin"}
print(f"Sending: {json.dumps(payload)}")
print("Response: 201 Created")
```

---

# Error Handling
<!-- timing: 1.5 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Success (2xx)**

```python
status = 200
print(f"OK: {status}")
```

<!-- column: 1 -->

**Error (4xx/5xx)**

```python
status = 403
print(f"DENIED: {status}")
```

<!-- reset_layout -->
````

## 3. Security Assessment Report

Red team report with attack/defense columns.

````markdown
---
title: Red Team Report Q4
theme: cyber_red
---

# Red Team Report
<!-- section: overview -->
<!-- timing: 2.0 -->
<!-- ascii_title -->

Q4 2025 Assessment Results

<!-- notes: Start with executive summary -->

---

# Scope
<!-- timing: 1.5 -->

| Target | Type | Result |
|:-------|:-----|:------:|
| Web App | External | 3 Critical |
| API | External | 1 Critical |
| AD | Internal | 2 High |
| Cloud | AWS | 1 High |

---

# Finding: SQLi
<!-- section: findings -->
<!-- timing: 3.0 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Attack**
- Parameter: `search_query`
- Type: Union-based SQLi
- Impact: Full DB access

```sql {label: "payload"}
' UNION SELECT username,
password FROM users--
```

<!-- column: 1 -->

**Defense**
- Use parameterized queries
- Input validation
- WAF rules

```python {label: "fix"}
cursor.execute(
    "SELECT * FROM items "
    "WHERE name = %s",
    (user_input,)
)
```

<!-- reset_layout -->

<!-- notes: Emphasize this was the most critical finding -->

---

# Finding: SSRF
<!-- timing: 2.0 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Attack Vector**
- Image proxy endpoint
- No URL validation
- Access to metadata API

<!-- column: 1 -->

**Remediation**
- Allowlist domains
- Block RFC1918 ranges
- Disable metadata v1

<!-- reset_layout -->

> IMDSv2 with hop limit = 1 prevents container-based SSRF

---

# Remediation Summary
<!-- section: summary -->
<!-- timing: 1.0 -->

| Severity | Found | Fixed | Remaining |
|:---------|:-----:|:-----:|:---------:|
| Critical | 4     | 2     | **2**     |
| High     | 3     | 1     | **2**     |
| Medium   | 8     | 6     | 2         |

<!-- notes: Push for 30-day remediation deadline on criticals -->
````

## 4. Image Gallery

Showcasing images with different render modes.

````markdown
---
title: Architecture Overview
theme: blueprint
---

# System Design
<!-- timing: 2.0 -->

![High-level architecture](assets/architecture.png)
<!-- image_scale: 70 -->

> Three-tier architecture with message queue

---

# Network Topology
<!-- timing: 2.0 -->

![Network diagram](assets/network.png)
<!-- image_render: kitty -->
<!-- image_scale: 60 -->

---

# Dashboard
<!-- timing: 1.0 -->

![Monitoring dashboard](assets/dashboard.png)
<!-- image_render: ascii -->
<!-- image_scale: 80 -->

<!-- notes: ASCII render for tmux compatibility -->
````

## 5. Workshop / Hands-On

Interactive workshop with executable code at each step.

````markdown
---
title: Python Workshop
theme: catppuccin
---

# Python Workshop
<!-- section: start -->
<!-- timing: 1.0 -->
<!-- ascii_title -->

Hands-on coding session

<!-- notes: Make sure everyone has Python 3.10+ installed -->

---

# Step 1: Variables
<!-- section: basics -->
<!-- timing: 5.0 -->

```python +exec {label: "variables"}
name = "Workshop"
count = 42
pi = 3.14159
active = True

print(f"name: {name} ({type(name).__name__})")
print(f"count: {count} ({type(count).__name__})")
print(f"pi: {pi} ({type(pi).__name__})")
print(f"active: {active} ({type(active).__name__})")
```

<!-- notes: Give participants 3 minutes to try their own variables -->

---

# Step 2: Lists
<!-- timing: 5.0 -->

```python +exec {label: "list operations"}
fruits = ["apple", "banana", "cherry"]
fruits.append("date")
fruits.sort()

for i, fruit in enumerate(fruits):
    print(f"{i}: {fruit}")

print(f"\nTotal: {len(fruits)} fruits")
```

---

# Step 3: Functions
<!-- section: intermediate -->
<!-- timing: 5.0 -->

```python +exec {label: "defining functions"}
def greet(name, greeting="Hello"):
    return f"{greeting}, {name}!"

def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)

print(greet("Workshop"))
print(greet("Python", "Welcome to"))
print(f"5! = {factorial(5)}")
```

---

# Step 4: Error Handling
<!-- timing: 5.0 -->

<!-- column_layout: [1, 1] -->
<!-- column: 0 -->

**Without handling**

```python {label: "crashes"}
# This would crash:
# result = 10 / 0
print("Unhandled = crash")
```

<!-- column: 1 -->

**With handling**

```python +exec {label: "safe"}
try:
    result = 10 / 0
except ZeroDivisionError:
    print("Caught division by zero!")
finally:
    print("Cleanup done")
```

<!-- reset_layout -->

---

# Wrap Up
<!-- section: closing -->
<!-- timing: 1.0 -->

- Variables and types
- Lists and iteration
- Functions and recursion
- Error handling with `try`/`except`

> Practice makes perfect - try modifying the examples!
````

## Tips for Each Style

| Style | Key Techniques |
|-------|---------------|
| Simple Talk | Use sections, timing, and speaker notes liberally |
| Technical Demo | Heavy use of `+exec`, keep code blocks focused |
| Security Report | Column layouts for attack/defense, tables for summaries |
| Image Gallery | Per-slide `image_render` and `image_scale` directives |
| Workshop | `+exec` on every code block, generous timing values |
