# Docker Build and Deployment Guide

## Local Build and Test

### Build Docker Image
```bash
cd shadowcrawl
docker build -t shadowcrawl-mcp:latest .
```

### Test the Image
```bash
# Run HTTP server (port 5000)
docker run --rm \
  -e SEARXNG_URL=http://localhost:8888 \
  -e RUST_LOG=info \
  -p 5000:5000 \
  shadowcrawl-mcp:latest

# Or run MCP stdio server
docker run --rm -it \
  -e SEARXNG_URL=http://localhost:8888 \
  shadowcrawl-mcp:latest \
  shadowcrawl-mcp
```

### Run with docker-compose
```bash
# Start all services (SearXNG + MCP Server)
docker-compose up -d

# Check logs
docker-compose logs shadowcrawl

# Stop all services
docker-compose down
```

## GitHub Actions Auto-Deployment

### How It Works
The project uses GitHub Actions to automatically build and push Docker images to GitHub Container Registry (ghcr.io).

**Trigger Conditions:**
- ✅ **Manual trigger**: Run workflow manually from GitHub Actions tab
- ✅ **Commit with `[build]`**: Any commit message containing `[build]` will trigger the build
- ❌ **Other commits**: Regular commits without `[build]` will NOT trigger builds

### Usage Examples

#### Trigger Build
```bash
# Commit with [build] tag to trigger Docker build
git commit -m "[build] Add new feature"
git push

# Or include [build] anywhere in commit message
git commit -m "Fix bug - deploy [build] to production"
git push
```

#### Skip Build
```bash
# Normal commits without [build] will skip Docker build
git commit -m "Update docs"
git push

git commit -m "Fix typo"
git push
```

### Manual Workflow Dispatch
1. Go to your repository on GitHub
2. Click **Actions** tab
3. Select **Build and Push Docker Image** workflow
4. Click **Run workflow** button
5. Select branch and click **Run workflow**

### Access Published Images

Images are published to GitHub Container Registry:
```bash
# Pull the latest image
docker pull ghcr.io/YOUR_USERNAME/shadowcrawl:latest

# Pull specific version by commit SHA
docker pull ghcr.io/YOUR_USERNAME/shadowcrawl:main-abc1234

# Pull specific branch
docker pull ghcr.io/YOUR_USERNAME/shadowcrawl:main
```

**Note:** Replace `YOUR_USERNAME` with your GitHub username (lowercase).

### Multi-Architecture Support
The workflow builds for both:
- `linux/amd64` (Intel/AMD processors)
- `linux/arm64` (ARM processors like Apple Silicon, Raspberry Pi)

### Image Tags
Each build creates multiple tags:
- `latest` - Latest build from default branch
- `main` - Latest build from main branch  
- `main-abc1234` - Specific commit SHA
- `v1.0.0` - Semantic version tags (if using git tags)

### Environment Variables
Configure the container with:
```bash
docker run -e SEARXNG_URL=http://searxng:8080 \
           -e QDRANT_URL=http://qdrant:6334 \
           -e RUST_LOG=info \
           -e MAX_CONTENT_CHARS=10000 \
           ghcr.io/YOUR_USERNAME/shadowcrawl:latest
```

## Troubleshooting

### Build Failures
Check GitHub Actions logs:
1. Go to **Actions** tab
2. Click on failed workflow run
3. Expand failed job to see error details

### Permission Issues
If you get "permission denied" errors:
1. Go to **Settings** → **Actions** → **General**
2. Under "Workflow permissions", select "Read and write permissions"
3. Save and re-run the workflow

### Image Not Found
Make sure the container registry package is public:
1. Go to your repository
2. Click **Packages** on the right sidebar
3. Click on your package
4. Go to **Package settings**
5. Under "Danger Zone", click "Change visibility" → "Public"

## CI/CD Best Practices

### Development Workflow
```bash
# Regular development - no build
git commit -m "WIP: implementing feature"
git push

# Ready for deployment - trigger build
git commit -m "[build] Release v1.2.0 with new features"
git push
```

### Version Tagging
```bash
# Create version tag to trigger versioned build
git tag v1.0.0
git push origin v1.0.0

# This creates images with tags: v1.0.0, v1.0, v1, latest
```

### Rollback Strategy
```bash
# Pull and run specific version
docker pull ghcr.io/YOUR_USERNAME/shadowcrawl:main-abc1234
docker run ghcr.io/YOUR_USERNAME/shadowcrawl:main-abc1234
```

## Security Notes

- The workflow uses `GITHUB_TOKEN` automatically provided by GitHub Actions
- No manual token configuration needed
- Images are scanned for vulnerabilities
- Use environment variables for sensitive configuration
- Never commit secrets to repository
