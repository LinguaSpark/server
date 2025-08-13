# mtranservercore-rs

## BergaRust - Translation Service

[![GitHub Repo](https://img.shields.io/badge/GitHub-Repository-blue.svg)](https://github.com/LinguaSpark/server)
[![Docker Image](https://img.shields.io/badge/Docker-Image-blue.svg)](https://github.com/LinguaSpark/server/pkgs/container/translation-service)

A lightweight multilingual translation service based on Rust and Bergamot translation engine, compatible with multiple translation frontend APIs.

[简体中文](README_ZH.md)

## Project Background

This project originated when I discovered the [MTranServer](https://github.com/xxnuo/MTranServer/) repository, which uses [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/) for machine translation and is compatible with APIs like Immersive Translate and Kiss Translator, but found that it wasn't open-sourced yet.

While searching for similar projects, I found Mozilla's [translation-service](https://github.com/mozilla/translation-service/), which works but hasn't been updated for a year and isn't compatible with Immersive Translate or Kiss Translator APIs. Since that project is written in C++ and I'm not very familiar with C++, I rewrote this project in Rust.

## Features

- 💪 Written in Rust for excellent performance and low memory footprint
- 🔄 Based on [Bergamot Translator](https://github.com/browsermt/bergamot-translator) engine used in Firefox
- 🧠 Compatible with [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/)
- 🔍 Built-in language detection with automatic source language identification
- 🔌 Supports multiple translation API formats:
  - Native API
  - [Immersive Translate](https://immersivetranslate.com/) API
  - [Kiss Translator](https://www.kis-translator.com/) API
  - [HCFY](https://hcfy.app/) API
  - [DeepLX](https://github.com/OwO-Network/DeepLX) API
- 🔑 API key protection support
- 🐳 Docker deployment ready

## Tech Stack

- **Web Framework**: [Axum](https://github.com/tokio-rs/axum)
- **Translation Engine**: [Bergamot Translator](https://github.com/browsermt/bergamot-translator)
- **Translation Models**: [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/)
- **Language Detection**: [Whichlang](https://github.com/quickwit-oss/whichlang)

## Deployment

Docker is the **only recommended** deployment method for this service.

### Option 1: Using pre-built image (with your own translation models)

```bash
# Create models directory
mkdir -p models
# Download your models here
# Pull and start container
docker run -d --name translation-service \
  -p 3000:3000 \
  -v "$(pwd)/models:/app/models" \
  ghcr.io/linguaspark/server:main
```

### Option 2: Using pre-built image with English-Chinese model (China mirror)

```bash
docker run -d --name translation-service \
  -p 3000:3000 \
  docker.cnb.cool/linguaspark/server:latest
```

> Note: The English-Chinese model image is about 70MiB, and each worker uses approximately 300MiB+ of memory with low translation latency.

### Docker Compose Deployment

Create a `compose.yaml` file:

```yaml
services:
  translation-service:
    image: ghcr.io/linguaspark/server:main
    ports:
      - "3000:3000"
    volumes:
      - ./models:/app/models
    environment:
      API_KEY: "your_api_key"  # Optional, leave empty to disable API key protection
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "/bin/sh", "-c", "echo -e 'GET /health HTTP/1.1\r\nHost: localhost:3000\r\n\r\n' | timeout 5 bash -c 'cat > /dev/tcp/localhost/3000' && echo 'Health check passed'"]
      interval: 30s
      timeout: 10s
      retries: 3
```

Start the service:

```bash
docker compose up -d
```

### Custom Image for Specific Language Pairs

If you need to create a custom image with specific language pairs, use this Dockerfile template:

```dockerfile
FROM ghcr.io/linguaspark/server:main

COPY ./your-models-directory /app/models

ENV MODELS_DIR=/app/models
ENV NUM_WORKERS=1
ENV IP=0.0.0.0
ENV PORT=3000
ENV RUST_LOG=info

EXPOSE 3000

ENTRYPOINT ["/app/server"]
```

## Translation Models

### Getting Models

1. Download pre-trained models from [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/)
2. Place them in the models directory with the following structure:

```
models/
├── enzh/  # Language pair directory name format: "[source language code][target language code]"
│   ├── model.intgemm8.bin  # Translation model
│   ├── model.s2t.bin       # Shortlist file
│   ├── srcvocab.spm        # Source language vocabulary
│   └── trgvocab.spm        # Target language vocabulary
└── zhen/  # Another language pair
    └── ...
```

### Language Pair Support

The translation service will automatically scan all language pair directories under the `models` directory and load them. Directory names should follow the `[source language][target language]` format using [ISO 639-1](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes) language codes.

## Environment Variables

| Variable Name | Description | Default Value |
|---------------|-------------|---------------|
| `MODELS_DIR`  | Path to models directory | `/app/models` |
| `NUM_WORKERS` | Number of translation worker threads | `1` |
| `IP`          | IP address for the service to listen on | `127.0.0.1` |
| `PORT`        | Port for the service to listen on | `3000` |
| `API_KEY`     | API key (leave empty to disable) | `""` |
| `RUST_LOG`    | Log level | `info` |

## API Endpoints

### Native API

#### Translate

```
POST /translate
```

Request body:
```json
{
  "text": "Hello world",
  "from": "en",  // Optional, omit to auto-detect
  "to": "zh"
}
```

Response:
```json
{
  "text": "你好世界",
  "from": "en",
  "to": "zh"
}
```

#### Language Detection

```
POST /detect
```

Request body:
```json
{
  "text": "Hello world"
}
```

Response:
```json
{
  "language": "en"
}
```

### Compatible APIs

#### Immersive Translate API

```
POST /imme
```

Request body:
```json
{
  "source_lang": "auto",  // Optional, omit to auto-detect
  "target_lang": "zh",
  "text_list": ["Hello world", "How are you?"]
}
```

Response:
```json
{
  "translations": [
    {
      "detected_source_lang": "en",
      "text": "你好世界"
    },
    {
      "detected_source_lang": "en",
      "text": "你好吗？"
    }
  ]
}
```

#### Kiss Translator API

```
POST /kiss
```

Request body:
```json
{
  "text": "Hello world",
  "from": "en",  // Optional, omit to auto-detect
  "to": "zh"
}
```

Response:
```json
{
  "text": "你好世界",
  "from": "en",
  "to": "zh"
}
```

#### HCFY API

```
POST /hcfy
```

Request body:
```json
{
  "text": "Hello world",
  "source": "英语",  // Optional, omit to auto-detect
  "destination": ["中文(简体)"]
}
```

Response:
```json
{
  "text": "Hello world",
  "from": "英语",
  "to": "中文(简体)",
  "result": ["你好世界"]
}
```

#### DeepLX API

```
POST /deeplx
```

Request body:
```json
{
  "text": "Hello world",
  "source_lang": "EN",
  "target_lang": "ZH"
}
```

Response:
```json
{
  "code": 200,
  "id": 1744646400,
  "data": "你好世界",
  "alternatives": [],
  "source_lang": "EN",
  "target_lang": "ZH"
}
```

### Health Check

```
GET /health
```

Response:
```json
{
  "status": "ok"
}
```

## Authentication

If the `API_KEY` environment variable is set, all API requests must provide authentication credentials using one of the following methods:

1. Authorization header: `Authorization: Bearer your_api_key`
2. Query parameter: `?token=your_api_key`

## License

This project is open-sourced under the AGPL-3.0 license.

## Acknowledgements

- [Bergamot Translator](https://github.com/browsermt/bergamot-translator) - Translation engine
- [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/) - Translation models
- [MTranServer](https://github.com/xxnuo/MTranServer/) - Inspiration
- [Mozilla Translation Service](https://github.com/mozilla/translation-service/) - Reference implementation
