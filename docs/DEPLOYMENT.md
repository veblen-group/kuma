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
|  |  Caddy    | :443 -> :3000 (reverse proxy +    |
|  |           |         auto Let's Encrypt TLS)   |
|  +-----+----+                                    |
|        |                                         |
|  +-----v------+  BACKEND_URL  +--------------+   |
|  |  frontend   |------------->| kuma-backend  |  |
|  |  :3000      | (Docker DNS) | :8080         |  |
|  +-------------+              +-------+-------+  |
|                                       |          |
|  +-------------+              private IP         |
|  |   kumad      |------------>| Cloud SQL    |   |
|  |  (optional)  |             | :5432        |   |
|  +--------------+             +--------------+   |
|                                        |         |
+----------------------------------------|---------+
                                         |
                                         v
                                  +--------------+
                                  |  Cloud SQL   |
                                  |  PostgreSQL  |
                                  |  (managed)   |
                                  +--------------+
```

**Public:** Only Caddy (ports 80/443) is exposed to the internet.
Backend, kumad, and the database are internal-only.

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

Enable the private IP for Cloud SQL and note the **private IP address** — this
is what `KUMA_DATABASE__HOST` must be set to in `docker-compose.prod.yml`:

```bash
gcloud sql instances describe kuma-db --format="value(ipAddresses)"
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

### 4. Reserve a static IP and assign to the VM

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

### 5. Firewall rules

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

### 6. DNS

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

From your **local machine**, copy the required files to the VM:

```bash
# Create the app directory on the VM
gcloud compute ssh kuma-vm --zone=us-east1-b -- "mkdir -p /home/$USER/kuma"

# Copy files (from the repo root)
gcloud compute scp \
  docker-compose.prod.yml \
  Caddyfile \
  kuma.yaml \
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

### Configure files on the VM

SSH in and edit the configuration:

```bash
gcloud compute ssh kuma-vm --zone=us-east1-b
cd ~/kuma
```

1. **Edit `Caddyfile`** -- replace `yourdomain.com` with your actual domain.

2. **Edit `kuma.yaml`** -- ensure database credentials match your Cloud SQL
   user/password and that private keys and RPC URLs are set.

3. **Edit `docker-compose.prod.yml`** -- set `KUMA_DATABASE__HOST` in the
   `kuma-backend` service to the Cloud SQL private IP address.

4. **Create `.env`** with the required environment variables:

```bash
cat > .env << 'EOF'
PGPASSWORD=YOUR_DB_PASSWORD
EOF
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
docker compose -f docker-compose.prod.yml --profile webapp pull

# Start core services (caddy, frontend, backend)
docker compose -f docker-compose.prod.yml --profile webapp up -d
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
docker compose -f docker-compose.prod.yml --profile webapp ps
docker compose -f docker-compose.prod.yml --profile webapp logs --tail=20
```

## Ongoing deploys

When you fix a bug or add a feature:

```bash
# 1. SSH into the VM
gcloud compute ssh kuma-vm --zone=us-east1-b
cd ~/kuma

# 2. Pull and restart core services
docker compose -f docker-compose.prod.yml --profile webapp pull
docker compose -f docker-compose.prod.yml --profile webapp up -d

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

### Backend can't reach the database

- Verify `KUMA_DATABASE__HOST` in `docker-compose.prod.yml` matches the Cloud SQL private IP
- Verify Cloud SQL private IP connectivity from the VM: `nc -zv <CLOUD_SQL_PRIVATE_IP> 5432`
- Verify database credentials in `kuma.yaml` match the Cloud SQL user

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
- **Secrets management**: Move secrets from `kuma.yaml` to GCP Secret Manager
  and inject them as environment variables.
