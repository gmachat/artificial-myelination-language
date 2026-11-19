# Inventory manager benchmark

Build a complete inventory web application.

Required record fields:

- `name`: non-empty text
- `quantity`: integer
- `active`: boolean

Required behavior:

- A browser page lists products.
- A form creates and updates a product.
- A control deletes a product.
- The API uses GET and POST at `/api/products` and PUT and DELETE at
  `/api/products/:id`.
- Invalid types, missing fields, empty text, and unknown fields are rejected.
- Records receive unique identifiers and persist across server restarts.
- The project provides development, production-build, and production-start commands.
- The production server hosts both the API and compiled browser application.

Return only one JSON object with a `files` object. Each key is a relative file
path and each value is the complete UTF-8 file content. Do not include lockfiles,
dependency directories, generated assets, markdown fences, or commentary.

