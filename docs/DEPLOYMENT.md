# GCP Deployment Guide

Deploy the kuma monorepo to a single GCP Compute Engine VM with Cloud SQL
for managed PostgreSQL and Caddy for automatic HTTPS.

## Architecture

```
Internet
  |
  | HTTPS (443)
  v
+--------------------------------------------------+
|  GCP e2-small VM (us-east1)                      |
|                                                  |
|  +----------+                                    |
|  |  Caddy    | :443 /api/* -> kuma-backend:8080  |
|  |           |       /*    -> kuma-webapp:3000   |
|  +-----+----+                                    |
|        |                                         |
|  +-----v------+              +--------------+    |
|  | kuma-webapp |             | kuma-backend |    |
|  |  :3000      |             | :8080        |    |
|  +-------------+             +-------+------+    |
|                                       |           |
|  +-------------+              +-------v-------+  |
|  |   kumad      |------------>| cloud-sql-    |  |
|  |  (optional)  | (Docker DNS)|   proxy :5432 |  |
|  +--------------+             +-------+-------+  |
|                                       |          |
+---------------------------------------|----------+
                                        | (IAM auth via service account)
                                        v
                                 +--------------+
                                 |  Cloud SQL   |
                                 |  PostgreSQL  |
                                 |  (managed)   |
                                 +--------------+
```

**Public:** Only Caddy (ports 80/443) is exposed to the internet.
Backend, kumad, Cloud SQL Proxy, and the database are internal-only.

## Cost

| Resource                       | Monthly Cost |
|--------------------------------|-------------|
| e2-small VM (2 vCPU, 2 GB)    | ~$13        |
| 30 GB SSD persistent disk     | ~$3         |
| Static external IP             | ~$3         |
| Cloud SQL db-f1-micro (10 GB) | ~$9         |
| **Total**                      | **~$28/mo** |

See `docs/RESOURCE_USAGE.md` for detailed per-service RAM/CPU analysis.

## Prerequisites

- GCP project with billing enabled
- `gcloud` CLI installed and authenticated (`gcloud auth login`)
- A domain name with access to DNS settings
- This repo cloned locally

## One-time GCP setup

All commands below use a placeholder `YOUR_GCP_PROJECT_ID`. Replace it with
your actual project ID.

### 1. Set project and enable APIs

```bash
gcloud config set project YOUR_GCP_PROJECT_ID

gcloud services enable \
  compute.googleapis.com \
  sqladmin.googleapis.com
```

### 2. Create Cloud SQL instance

```bash
gcloud sql instances create kuma-db \
  --database-version=POSTGRES_15 \
  --tier=db-f1-micro \
  --region=us-east1 \
  --storage-size=10GB \
  --storage-type=SSD \
  --backup-start-time=04:00 \
  --availability-type=zonal
```

Create the database and user:

```bash
gcloud sql databases create api_db --instance=kuma-db

gcloud sql users create api_user \
  --instance=kuma-db \
  --password=YOUR_DB_PASSWORD
```

Note the **instance connection name** (format: `YOUR_PROJECT:REGION:INSTANCE`) —
you'll need it for the `.env` file:

```bash
gcloud sql instances describe kuma-db --format="value(connectionName)"
```

### 3. Create the VM

```bash
gcloud compute instances create kuma-vm \
  --zone=us-east1-b \
  --machine-type=e2-small \
  --image-family=debian-12 \
  --image-project=debian-cloud \
  --boot-disk-size=30GB \
  --boot-disk-type=pd-ssd \
  --tags=http-server,https-server \
  --scopes=cloud-platform
```

The `--scopes=cloud-platform` flag gives the VM's service account access to
GCP APIs, including Cloud SQL Auth Proxy authentication.

### 4. Grant Cloud SQL client role to the VM's service account

The Cloud SQL Auth Proxy uses the VM's service account to authenticate. Grant
it the required role:

```bash
SA=$(gcloud compute instances describe kuma-vm \
  --zone=us-east1-b \
  --format="value(serviceAccounts[0].email)")

gcloud projects add-iam-policy-binding YOUR_GCP_PROJECT_ID \
  --member="serviceAccount:${SA}" \
  --role="roles/cloudsql.client"
```

### 5. Reserve a static IP and assign to the VM

```bash
gcloud compute addresses create kuma-ip --region=us-east1

# Get the static IP
STATIC_IP=$(gcloud compute addresses describe kuma-ip \
  --region=us-east1 --format="value(address)")
echo "Static IP: ${STATIC_IP}"

# Remove the ephemeral IP and assign the static one
gcloud compute instances delete-access-config kuma-vm \
  --zone=us-east1-b \
  --access-config-name="external-nat"

gcloud compute instances add-access-config kuma-vm \
  --zone=us-east1-b \
  --address="${STATIC_IP}"
```

### 6. Firewall rules

```bash
gcloud compute firewall-rules create allow-http \
  --allow=tcp:80 \
  --target-tags=http-server \
  --description="Allow HTTP for Caddy ACME challenge"

gcloud compute firewall-rules create allow-https \
  --allow=tcp:443 \
  --target-tags=https-server \
  --description="Allow HTTPS traffic"
```

### 7. DNS

Add an **A record** for your domain pointing to the static IP:

```
yourdomain.com  A  <STATIC_IP>
```

Wait for DNS propagation before proceeding (Caddy needs to reach your domain
to provision the TLS certificate).

## One-time VM setup

SSH into the VM:

```bash
gcloud compute ssh kuma-vm --zone=us-east1-b
```

### Install Docker

```bash
# Install Docker
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# Log out and back in for group change to take effect
exit
gcloud compute ssh kuma-vm --zone=us-east1-b

# Verify
docker --version
docker compose version
```

### Create app directory and copy config files

From your **local machine**:

```bash
# Create the app directory on the VM
gcloud compute ssh kuma-vm --zone=us-east1-b -- "mkdir -p /home/$USER/kuma"

# Copy compose file, Caddyfile, and token lists
gcloud compute scp \
  docker-compose.prod.yml \
  Caddyfile \
  tokens.ethereum.json \
  tokens.base.json \
  tokens.unichain.json \
  kuma-vm:/home/$USER/kuma/ \
  --zone=us-east1-b

# Copy migrations directory
gcloud compute scp --recurse \
  migrations/ \
  kuma-vm:/home/$USER/kuma/migrations/ \
  --zone=us-east1-b
```

### Configure secrets

Still from your **local machine**, fill in the secret files and push them:

**`kuma.prod.yaml`** — fill in RPC URLs, private keys, Tycho API key, and DB password:

```bash
just reset-prod-config   # creates kuma.prod.yaml from the example template
# edit kuma.prod.yaml with your values
just push-prod-config    # pushes to the VM
```

**`.env`** — fill in Cloud SQL connection name and DB password:

```bash
just reset-env           # creates .env from the example template
# edit .env with your values:
#   CLOUD_SQL_CONNECTION_NAME — get with: gcloud sql instances describe kuma-db --format="value(connectionName)"
#   PGPASSWORD — the password you set for api_user
just push-env            # pushes to the VM
```

**`Caddyfile`** — replace `yourdomain.com` with your actual domain, then push:

```bash
# edit Caddyfile
just push-caddyfile      # or: gcloud compute scp Caddyfile kuma-vm:/home/$USER/kuma/ --zone=us-east1-b
```

## First deploy

SSH into the VM:

```bash
gcloud compute ssh kuma-vm --zone=us-east1-b
cd ~/kuma
```

### Pull images and start core services

Images are hosted on GitHub Container Registry (`ghcr.io/veblen-group/`).

```bash
# Pull images
docker compose -f docker-compose.prod.yml --profile frontend pull

# Start core services (caddy, frontend, backend)
docker compose -f docker-compose.prod.yml --profile frontend up -d
```

### Run schema migration

```bash
docker compose -f docker-compose.prod.yml --profile init up schema-migration
```

### Start kumad (when ready)

```bash
docker compose -f docker-compose.prod.yml --profile kumad up -d
```

### Verify

Visit `https://yourdomain.com` -- you should see the kuma dashboard.

Check service health:

```bash
docker compose -f docker-compose.prod.yml --profile frontend ps
docker compose -f docker-compose.prod.yml --profile frontend logs --tail=20
```

## Ongoing deploys

When you fix a bug or add a feature:

```bash
# 1. SSH into the VM
gcloud compute ssh kuma-vm --zone=us-east1-b
cd ~/kuma

# 2. Pull and restart core services
docker compose -f docker-compose.prod.yml --profile frontend pull
docker compose -f docker-compose.prod.yml --profile frontend up -d

# If kumad is also running:
docker compose -f docker-compose.prod.yml --profile all pull
docker compose -f docker-compose.prod.yml --profile all up -d
```

## Managing kumad

kumad is under a separate Docker Compose profile so it can be started and
stopped independently of the core services.

```bash
# Start kumad
docker compose -f docker-compose.prod.yml --profile kumad up -d kumad

# Stop kumad (dashboard stays up)
docker compose -f docker-compose.prod.yml stop kumad

# View kumad logs
docker compose -f docker-compose.prod.yml logs -f kumad
```

## Monitoring

Set up a free GCP uptime check:

1. Go to **Cloud Monitoring > Uptime Checks** in the GCP Console
2. Create a new check:
   - Protocol: **HTTPS**
   - Hostname: **yourdomain.com**
   - Path: **/**
   - Check frequency: **5 minutes**
3. Add a notification channel (email) to alert on downtime

## Troubleshooting

### Caddy fails to get TLS certificate

- Verify DNS A record points to the VM's static IP: `dig yourdomain.com`
- Verify ports 80 and 443 are open: `gcloud compute firewall-rules list`
- Check Caddy logs: `docker compose -f docker-compose.prod.yml logs caddy`

### Cloud SQL Proxy fails to connect

- Verify `CLOUD_SQL_CONNECTION_NAME` in `.env` is correct: `gcloud sql instances describe kuma-db --format="value(connectionName)"`
- Verify the VM service account has `roles/cloudsql.client`: `gcloud projects get-iam-policy YOUR_GCP_PROJECT_ID --flatten="bindings[].members" --filter="bindings.role=roles/cloudsql.client"`
- Check proxy logs: `docker compose -f docker-compose.prod.yml logs cloud-sql-proxy`

### Backend can't reach the database

- Verify Cloud SQL Proxy is running: `docker compose -f docker-compose.prod.yml ps cloud-sql-proxy`
- Verify database credentials in `kuma.prod.yaml` match the Cloud SQL user
- Test connectivity: `docker compose -f docker-compose.prod.yml exec kuma-backend sh -c 'nc -z cloud-sql-proxy 5432'`

### Images fail to pull

- Verify the image tags exist on `ghcr.io/veblen-group/`
- If the packages are private, authenticate Docker: `echo $GHCR_TOKEN | docker login ghcr.io -u USERNAME --password-stdin`

## Future improvements

- **CI/CD**: Add a GitHub Actions workflow to build and push images on push to
  `main`.
- **DB backups to GCS**: Cloud SQL daily backups are enabled, but for extra
  safety, schedule `pg_dump` exports to a Cloud Storage bucket.
- **Log aggregation**: Forward container logs to Cloud Logging via the
  `gcplogs` Docker log driver.
- **Secrets management**: Move secrets from `kuma.prod.yaml` to GCP Secret Manager
  and inject them as environment variables.
