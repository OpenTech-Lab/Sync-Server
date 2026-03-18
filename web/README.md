# Custom Web page
This folder can be used for building a web page.

## How to use

```bash
# From inside the server/ directory
cp -r /path/to/your-web-repo/. web/

# or if using git submodules:
git submodule add https://github.com/you/web-repo web

# The web repo needs a Dockerfile at its root. Then:

docker compose --profile web build web
docker compose --profile web up -d
```