# ROSE (WIP)

ROSE - Rust Object SErver. A server that is supposed to store a lot of files. Now only supports JPEG.

Endpoints:

```HTTP
GET /images/{UUID}
```

```HTTP
POST /images/{UUID}
```

```HTTP
DELETE /images/{UUID}
```

```HTTP
POST /api/update_cache
Content-Type: application/json
[
    "{UUID}", "{UUID}", ...
]
```

```.env``` variables that should be set:

- SERVER_ADDRESS
- IMAGES_DIR
- RUST_LOG (debug, info, trace, warn)
