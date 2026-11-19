# React target

AML's React target compiles a compact component description into a modern JSX
module. It is intended to remove repetitive component, state, event, layout,
and validation code while producing ordinary React source that existing tools
can bundle.

## Example

```aml
app counter
profile react

component Counter(title: Text):
  state count: Int = 0
  state name: Text = ""
  require title != ""

  view:
    column:
      heading title
      text "Count: {count}"
      when count == 0:
        text "Ready to begin"
      else:
        text "Nice work"
      button "Add one":
        on click:
          set count = count + 1
      input name
      text "Hello, {name}"
```

Build it:

```sh
aml check examples/counter.aml --target react
aml build examples/counter.aml --target react -o Counter.jsx
```

Copy or generate `Counter.jsx` into a React project's source directory, then
import it normally:

```jsx
import Counter from "./Counter.jsx";

export default function App() {
  return <Counter title="AML counter" />;
}
```

The generated component imports only `react`; there is no AML runtime package
in the browser bundle.

## React constructs in 0.8

- `component Name(props):` defines one exported component per AML file.
- `state name: Type = value` creates typed React state.
- `require condition` validates component inputs.
- `view:` contains one root UI element.
- `column:` and `row:` provide flex layouts with stable defaults.
- `heading expression` and `text expression` render content.
- `"Hello, {name}"` interpolates a simple name inside display text.
- `when condition:` renders its children only when the condition is true.
- `else:` optionally supplies the immediately preceding `when` alternative.
- `button expression:` contains one `on click:` event.
- `set state = expression` updates state.
- `show expression` inside an event writes to the browser console.
- `input state` creates a controlled text input.

The first React backend intentionally does not generate a full Vite or Next.js
project, manage npm dependencies, perform routing, or support arbitrary HTML.
Those are separate future layers. `aml run --target react` is therefore not
available; React output must be built inside a React toolchain.

