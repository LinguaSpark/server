# LinguaSpark - Translation Service

[![GitHub Repo](https://img.shields.io/badge/GitHub-Repository-blue.svg)](https://github.com/LinguaSpark/server)
[![Docker Image](https://img.shields.io/badge/Docker-Image-blue.svg)](https://github.com/LinguaSpark/server/pkgs/container/translation-service)

一个由纯 Rust LinguaSpark 推理引擎驱动的轻量级多语言翻译服务，兼容多种翻译前端 API。

[English](README.md)

## 项目背景

这个项目的起源是我看到了 [MTranServer](https://github.com/xxnuo/MTranServer/) 这个仓库，它使用了 [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/) 进行机器翻译，并且兼容了沉浸式翻译、简约翻译等 API，但发现它目前还没开源。

在寻找类似项目时，我发现了 Mozilla 的 [translation-service](https://github.com/mozilla/translation-service/)，虽然能用但有一年没更新了，也不兼容沉浸式翻译、简约翻译的 API。由于该项目是 C++ 编写的，而我对 C++ 不太熟悉，所以我使用 Rust 重新编写了这个项目。

## 功能特性

- 💪 使用 Rust 编写，性能优异，内存占用低
- 🔄 使用纯 Rust [LinguaSpark](https://github.com/LinguaSpark/linguaspark) 推理引擎
- 🧠 兼容 [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/)
- 🔍 内置语言检测，支持自动识别源语言
- 🔌 支持多种翻译前端 API 格式:
  - 原生 API
  - [沉浸式翻译 (Immersive Translate)](https://immersivetranslate.com/) API
  - [简约翻译 (Kiss Translator)](https://www.kis-translator.com/) API
  - [划词翻译 (HCFY)](https://hcfy.app/) API
  - [DeepLX](https://github.com/OwO-Network/DeepLX) API
- 🔑 支持 API 密钥保护
- 🐳 提供 Docker 镜像，便于部署

## 技术栈

- **Web 框架**: [Axum](https://github.com/tokio-rs/axum)
- **翻译引擎**: [LinguaSpark](https://github.com/LinguaSpark/linguaspark)
- **翻译模型**: [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/)
- **语言检测**: [Whichlang](https://github.com/quickwit-oss/whichlang)

## 部署

Docker 是本服务**唯一推荐**的部署方式。

### 方式一：使用自带英译中模型的镜像（国内托管，推荐，速度快）

```bash
docker run -d --name translation-service \
  -p 3000:3000 \
  docker.cnb.cool/aalivexy/translation-service:latest
```

### 方式二：使用预构建镜像（不含翻译模型）

```bash
# 创建模型目录
mkdir -p models
# 下载你的模型到目录里
# 拉取并启动容器
docker run -d --name translation-service \
  -p 3000:3000 \
  -v "$(pwd)/models:/app/models" \
  ghcr.io/linguaspark/server:main
```

### Docker Compose 部署

创建 `compose.yaml` 文件：

```yaml
services:
  translation-service:
    image: docker.cnb.cool/aalivexy/translation-service:latest
    ports:
      - "3000:3000"
    environment:
      API_KEY: "" # 可选，设置为空字符串则不启用 API 密钥保护
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "/bin/sh", "-c", "echo -e 'GET /health HTTP/1.1\r\nHost: localhost:3000\r\n\r\n' | timeout 5 bash -c 'cat > /dev/tcp/localhost/3000' && echo 'Health check passed'"]
      interval: 30s
      timeout: 10s
      retries: 3
```

启动服务：

```bash
docker compose up -d
```

### 自定义特定语言对的镜像

如果需要创建包含特定语言对的自定义镜像，可以使用以下 Dockerfile 模板：

```dockerfile
FROM ghcr.io/linguaspark/server:main

COPY ./your-models-directory /app/models

ENV MODELS_DIR=/app/models
ENV IP=0.0.0.0
ENV PORT=3000
ENV RUST_LOG=info

EXPOSE 3000

ENTRYPOINT ["/app/linguaspark-server"]
```

## 翻译模型

### 获取模型

1. 从 [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/) 下载预训练模型
2. 模型放置结构应为：

```
models/
├── en-zh/  # 同时接受 "en-zh" 和旧版 "enzh" 形式
│   ├── model.enzh.intgemm.alphas.bin.gz
│   ├── lex.50.50.enzh.s2t.bin.gz
│   ├── srcvocab.enzh.spm.gz
│   └── trgvocab.enzh.spm.gz
└── zhen/  # 另一个语言对
    └── ...
```

模型资产既可以保留 `.gz` 压缩，也可以使用已解压文件。只有一个 `vocab*.spm[.gz]` 的共享词表模型同样受支持。

### 语言对支持

翻译服务会自动扫描 `models` 目录下的语言对目录并加载模型。目录名必须使用 `enzh` 或 `en-zh` 形式，并采用 [ISO 639-1](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes) 语言代码。

## 环境变量

| 变量名 | 描述 | 默认值 |
|--------|------|--------|
| `MODELS_DIR` | 模型目录路径 | `/app/models` |
| `IP` | 服务监听的 IP 地址 | `127.0.0.1` |
| `PORT` | 服务监听的端口 | `3000` |
| `API_KEY` | API 密钥（留空则不启用） | `""` |
| `RUST_LOG` | 日志级别 | `info` |

## API 端点

### 原生 API

#### 翻译

```
POST /translate
```

请求体：
```json
{
  "text": "Hello world",
  "from": "en",  // 可选，省略则自动检测
  "to": "zh"
}
```

响应：
```json
{
  "text": "你好世界",
  "from": "en",
  "to": "zh"
}
```

#### 语言检测

```
POST /detect
```

请求体：
```json
{
  "text": "Hello world"
}
```

响应：
```json
{
  "language": "en"
}
```

### 兼容 API

#### 沉浸式翻译 API

```
POST /imme
```

请求体：
```json
{
  "source_lang": "auto",  // 可选，省略则自动检测
  "target_lang": "zh",
  "text_list": ["Hello world", "How are you?"]
}
```

响应：
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

#### 简约翻译 API

```
POST /kiss
```

请求体：
```json
{
  "text": "Hello world",
  "from": "en",  // 可选，省略则自动检测
  "to": "zh"
}
```

响应：
```json
{
  "text": "你好世界",
  "from": "en",
  "to": "zh"
}
```

#### 划词翻译 API

```
POST /hcfy
```

请求体：
```json
{
  "text": "Hello world",
  "source": "英语",  // 可选，省略则自动检测
  "destination": ["中文(简体)"]
}
```

响应：
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

请求体：
```json
{
  "text": "Hello world",
  "source_lang": "EN",  // 可选，省略则自动检测
  "target_lang": "ZH"
}
```

响应：
```json
{
  "code": 200,
  "id": 1744646400,
  "data": "你好世界",
  "alternatives": [],
  "source_lang": "EN",
  "target_lang": "ZH",
  "method": "Free"
}
```

### 健康检查

```
GET /health
```

响应：
```json
{
  "status": "ok"
}
```

## 认证

如果设置了 `API_KEY` 环境变量，除 `GET /health` 外的所有 API 请求都需要提供认证凭据，支持两种方式：

1. Authorization 头： `Authorization: Bearer your_api_key`
2. 查询参数： `?token=your_api_key`

## 许可证

本项目基于 AGPL-3.0 许可证开源。

## 致谢

- [LinguaSpark](https://github.com/LinguaSpark/linguaspark) - 提供纯 Rust 翻译推理
- [Firefox Translations Models](https://github.com/mozilla/firefox-translations-models/) - 提供翻译模型
- [MTranServer](https://github.com/xxnuo/MTranServer/) - 提供灵感来源
- [Mozilla Translation Service](https://github.com/mozilla/translation-service/) - 提供参考实现
