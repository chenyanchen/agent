#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if [[ ! -f .env ]]; then
  minio_password="$(openssl rand -hex 24)"
  postgres_password="$(openssl rand -hex 24)"
  cat > .env <<EOF
NEXTAUTH_URL=http://localhost:3000
NEXTAUTH_SECRET=$(openssl rand -hex 32)
SALT=$(openssl rand -hex 32)
ENCRYPTION_KEY=$(openssl rand -hex 32)
POSTGRES_PASSWORD=${postgres_password}
DATABASE_URL=postgresql://postgres:${postgres_password}@postgres:5432/postgres
CLICKHOUSE_PASSWORD=$(openssl rand -hex 24)
REDIS_AUTH=$(openssl rand -hex 24)
MINIO_ROOT_PASSWORD=${minio_password}
LANGFUSE_S3_EVENT_UPLOAD_SECRET_ACCESS_KEY=${minio_password}
LANGFUSE_S3_MEDIA_UPLOAD_SECRET_ACCESS_KEY=${minio_password}
TELEMETRY_ENABLED=false
LANGFUSE_INIT_ORG_ID=agent-local
LANGFUSE_INIT_ORG_NAME=Agent Local
LANGFUSE_INIT_PROJECT_ID=agent-development
LANGFUSE_INIT_PROJECT_NAME=Agent Development
LANGFUSE_INIT_PROJECT_PUBLIC_KEY=pk-lf-$(openssl rand -hex 24)
LANGFUSE_INIT_PROJECT_SECRET_KEY=sk-lf-$(openssl rand -hex 24)
LANGFUSE_INIT_USER_EMAIL=agent@example.com
LANGFUSE_INIT_USER_NAME=Agent Developer
LANGFUSE_INIT_USER_PASSWORD=$(openssl rand -base64 24 | tr -d '/+=' | head -c 24)
EOF
  echo "Created dev/observability/.env"
fi

docker compose --env-file .env up -d

echo
echo "Langfuse: http://localhost:3000"
echo "Login email: $(grep '^LANGFUSE_INIT_USER_EMAIL=' .env | cut -d= -f2-)"
echo "Login password: $(grep '^LANGFUSE_INIT_USER_PASSWORD=' .env | cut -d= -f2-)"
echo
echo "Add this to ~/.agent/config.toml:"
echo "[observability]"
echo "enabled = true"
echo 'endpoint = "http://localhost:3000/api/public/otel/v1/traces"'
echo "public_key = \"$(grep '^LANGFUSE_INIT_PROJECT_PUBLIC_KEY=' .env | cut -d= -f2-)\""
echo "secret_key = \"$(grep '^LANGFUSE_INIT_PROJECT_SECRET_KEY=' .env | cut -d= -f2-)\""
