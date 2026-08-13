# Agent CLI

Agent CLI provides a terminal conversation interface in which users compose and submit messages to an AI agent.

## Language

**Self-hosting milestone**:
The point at which the primary user can use Agent CLI to develop and verify Agent itself through several real repository tasks, making further evolution a practical learning loop.
_Avoid_: Autonomous self-improvement, production parity, final product

**Skill**:
A locally discovered, reusable workflow whose instructions and resources guide Agent only when an explicit or implicit invocation selects it.
_Avoid_: Plugin, tool, permanent prompt

**Workdir**:
The directory selected when Agent CLI starts, anchoring project instructions, local skill discovery, and relative tool paths for that process.
_Avoid_: Git root, repository root, workspace object

**Canonical context**:
The provider-ready Responses item sequence used for the next model request. It may contain opaque compaction state and remains distinct from the human-readable transcript.
_Avoid_: Transcript, memory, summary

**Compaction**:
The server-side replacement of older canonical context with opaque state so a conversation can continue within its model context window without rewriting the transcript.
_Avoid_: Truncation, local summary

**Draft**:
The user-authored message being edited before submission. A draft may contain explicit line breaks and may be visually wrapped without changing its content.
_Avoid_: Input, prompt

**Soft wrap**:
A display-only line break introduced when a draft exceeds the available width; it is not part of the submitted message.
_Avoid_: Newline

**Explicit line break**:
A line break intentionally inserted into the draft, preserved in the submitted message, and shown as a visible line break in the conversation transcript.
_Avoid_: Soft wrap

**Draft viewport**:
The visible portion of the draft. It grows with the draft up to one third of the terminal height, then scrolls so every part of the draft remains reachable.
_Avoid_: Input line

**Chat message**:
A submitted user message or an Assistant response presented in the conversation transcript. Chat messages use CommonMark with supported GitHub Flavored Markdown extensions. Tool activity and errors are not chat messages and remain literal text.
_Avoid_: Chat entry
