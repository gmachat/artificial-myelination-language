# Full-stack target

AML 0.6 can generate a working React dashboard, REST API, runtime validation,
JSON or PostgreSQL persistence, development runner, migrations, and production
server from compact data and resource declarations.

```aml
app inventory_dashboard
profile fullstack

data Product:
  name: Text
  category: Text
  quantity: Int
  unit_price: Float
  active: Bool

resource products: Product:
  list
  create
  update
  delete

page Dashboard at "/":
  resource products
```

Generate and run it:

```sh
aml check examples/postgres_fullstack.aml --target fullstack
aml build examples/postgres_fullstack.aml --target fullstack -o build/dashboard
cd build/dashboard
npm install
npm run dev
```

The development command starts both the Node API and Vite frontend. For a
production-style run:

```sh
npm run build
npm start
```

The production Node server hosts the compiled frontend and API on one port.
Set `PORT` to change the port and `AML_DATA_FILE` to move the persistent store.

## PostgreSQL

Choose PostgreSQL explicitly and use `Time` for timestamp fields:

```aml
profile fullstack
storage postgres

data Game:
  starts_at: Time

resource games: Game:
  list
  create
  update
  delete
```

The generated project includes a numbered SQL migration and migration runner:

```sh
export DATABASE_URL="postgres://user:password@localhost:5432/database"
npm install
npm run migrate
npm run dev
```

PostgreSQL mode uses a bounded connection pool, parameterized values, validated
identifiers, transactional migration tracking, health checks, graceful pool
shutdown, and generated `created_at` and `updated_at` timestamps.

## Generated behavior

- REST list, create, and delete endpoints
- REST update endpoints and generated edit controls
- strict field and unknown-key validation
- generated UUID identifiers
- one-megabyte request-body limit
- serialized, atomic persistent-store mutations
- structured JSON success and failure responses
- generated React forms and responsive data tables
- loading, empty, and error states
- Vite development proxy
- production static-file hosting with SPA fallback
- health endpoint at `/api/health`

## Current boundary

PostgreSQL provides the durable data foundation for larger applications.
Relationships and indexes, authentication, multiple routes, scheduled jobs,
secrets, caching, charts, and deployment profiles remain future work.

