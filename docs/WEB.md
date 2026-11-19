# Full web application target

The `web` target turns one AML component into a complete Vite + React project.
It generates the browser entrypoint, HTML shell, resilient error boundary,
default styling, npm scripts, dependency manifest, and production build setup.

```sh
aml check examples/full_web.aml --target web
aml build examples/full_web.aml --target web -o build/aml-counter
cd build/aml-counter
npm install
npm run dev
```

Create a deployable production bundle with:

```sh
npm run build
```

The output in `dist/` can be hosted by any static web host. Preview it locally
with `npm run preview`.

## Generated structure

```text
aml-counter/
  package.json
  vite.config.js
  index.html
  README.md
  src/
    App.jsx
    ErrorBoundary.jsx
    main.jsx
    styles.css
```

AML refuses to write into a non-empty web output directory unless `--force` is
passed. Force mode updates generated files but does not delete unrelated files.

## Current boundary

Version 0.8 generates a complete client-side React application, including
typed state, events, controlled text input, and conditional `when`/`else`
views. It includes a
production frontend build, but it does not yet generate a server, database,
authentication, persistent storage, routing, or deployment account. Those
features require additional AML declarations rather than hidden assumptions.

