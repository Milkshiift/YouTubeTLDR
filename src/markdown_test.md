# Markdown Test

## 1. Text Formatting

This is *italic text* and this is _also italic_.
This is **bold text** and this is __also bold__.
This is ***bold and italic*** and this is ___also bold and italic___.
This is ~~strikethrough text~~.
This is `inline code`.

Here's a paragraph with a [link to Google](https://www.google.com).

## 2. Headings

# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6

## 3. Lists

### Unordered List

* Item 1
    * Sub-item A
    * Sub-item B
* Item 2
    * Sub-item C
        * Deep sub-item i
        * Deep sub-item ii
* Item 3

### Ordered List

1. First item
2. Second item
    1. Nested ordered item 1
    2. Nested ordered item 2
3. Third item
    * Mixed unordered item A
    * Mixed unordered item B

## 4. Blockquotes

> This is a blockquote.
> It can span multiple lines.
>
> > Nested blockquote!
>
> Back to the first level.

## 5. Code Blocks

### Inline Code

Here's some `print("hello world")` in Python.

### Fenced Code Block

```python
def factorial(n):
    if n == 0:
        return 1
    else:
        return n * factorial(n-1)

print(factorial(5))
```

### Indented Code Block

    This is an indented code block.
    It's typically indented by 4 spaces or 1 tab.
    Looks like pre-formatted text.

## 6. Tables

| Header 1 | Header 2 | Header 3 |
|:---------|:--------:|---------:|
| Left     |  Center  |    Right |
| Cell A   |  Cell B  |   Cell C |
| 123      |   456    |      789 |

## 7. Horizontal Rules

---

A line above this.

***

Another line above this.

___

And one more line above this.

## 8. Task Lists (GitHub Flavored Markdown)

- [x] Task 1 (completed)
- [ ] Task 2 (pending)
    - [x] Subtask A (completed)
    - [ ] Subtask B (pending)
- [ ] Task 3 (pending)

## 9. Backslash Escapes

\* Not an italic \*
\_ Not an italic \_
\` Not inline code \`
\# Not a heading \#
\[ Not a link \[

## 10. Definition Lists (Markdown Extra / Pandoc)

Term 1
: Definition of term 1.

Term 2
: Definition of term 2, line 1.
: Definition of term 2, line 2.