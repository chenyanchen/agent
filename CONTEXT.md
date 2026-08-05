# Agent CLI

Agent CLI provides a terminal conversation interface in which users compose and submit messages to an AI agent.

## Language

**Draft**:
The user-authored message being edited before submission. A draft may contain explicit line breaks and may be visually wrapped without changing its content.
_Avoid_: Input, prompt

**Soft wrap**:
A display-only line break introduced when a draft exceeds the available width; it is not part of the submitted message.
_Avoid_: Newline

**Explicit line break**:
A line break intentionally inserted into the draft and preserved in the submitted message.
_Avoid_: Soft wrap

**Draft viewport**:
The visible portion of the draft. It grows with the draft up to one third of the terminal height, then scrolls so every part of the draft remains reachable.
_Avoid_: Input line
