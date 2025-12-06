# Fastcache

キャッシングプロキシサーバー。ローカルファイル配信とリモートオリジンへのプロキシをサポートする。

## 特徴

- **ローカルファイル配信**: 静的ファイルをローカルファイルシステムから配信
- **リモートプロキシ**: 上流サーバーへのリバースプロキシ
- **フォールバック**: 同一パスに複数オリジンを設定し、順次フォールバック
- **インメモリキャッシュ**: TTL ベースキャッシュ
- **レート制限**: IP ベースのトークンバケットアルゴリズム
- **TLS/HTTPS**: ネイティブ TLS サポート（オプション）
- **ヘルスチェック**: 管理ポートの `/health` エンドポイント
- **管理用 Unix ソケット**: キャッシュのパージや統計取得

## 必要要件

- Rust 1.83 以上
- Docker（オプション）

## インストール

### ソースからビルド

```bash
cargo build --release
```

### Docker

```bash
make docker-build
```

## 使用方法

### サーバーの起動

```bash
# デフォルト設定で起動
cargo run -- serve

# カスタム設定ファイルで起動
cargo run -- serve --config /path/to/config.yaml
```

### キャッシュ管理

サーバー起動中に以下のコマンドでキャッシュを操作できる。

```bash
# 特定パスのキャッシュを削除
cargo run -- purge /path/to/file

# プレフィックスで一括削除
cargo run -- purge --prefix /static/

# 全キャッシュを削除
cargo run -- purge --all

# キャッシュ統計を表示
cargo run -- stats
```

### Docker Compose

```bash
# ビルドと起動
make docker-up

# ログ確認
make docker-logs

# 停止
make docker-down
```

## 設定

`config.yaml` で設定を行う。

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  admin_port: 9090              # 管理用ポート（ヘルスチェック）
  socket: "/tmp/fastcache.sock" # 管理用ソケット

cache:
  max_capacity: 10000   # 最大キャッシュエントリ数
  default_ttl: 3600     # デフォルト TTL（秒）

rate_limit:
  requests_per_second: 100
  burst_size: 50

# TLS 設定（オプション）
# tls:
#   cert_path: "/path/to/cert.pem"
#   key_path: "/path/to/key.pem"

origins:
  # ローカルファイル配信
  - path: "/static"
    type: local
    root: "./public"
    cache_ttl: 86400

  # リモートオリジンへのプロキシ
  - path: "/api"
    type: remote
    url: "http://backend:3000"
    cache_ttl: 0  # キャッシュなし
```

### オリジン設定

| パラメータ | 説明 |
|-----------|------|
| `path` | マッチするURLパス（最長プレフィックスマッチング） |
| `type` | `local`（ファイル配信）または `remote`（プロキシ） |
| `root` | ローカルオリジンのルートディレクトリ |
| `url` | リモートオリジンのURL |
| `cache_ttl` | キャッシュ有効期間（秒）。0でキャッシュ無効 |

### フォールバック

同一パスに複数のオリジンを設定すると、設定順にオリジンを試行する。最初のオリジンが失敗した場合、次のオリジンにフォールバックする。

```yaml
origins:
  # 1番目: ローカルファイルを優先
  - path: "/static"
    type: local
    root: "./public"
    cache_ttl: 86400

  # 2番目: ローカルにない場合はリモートから取得
  - path: "/static"
    type: remote
    url: "https://cdn.example.com"
    cache_ttl: 3600
```

この例では `/static/foo.txt` へのリクエストは:
1. `./public/foo.txt` を探す
2. 存在すればローカルから返却
3. 存在しなければ `https://cdn.example.com/foo.txt` から取得

## API

### ヘルスチェック

管理ポート（デフォルト: 9090）で提供。

```
GET http://localhost:9090/health
```

レスポンス例:
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

### レスポンスヘッダー

| ヘッダー | 説明 |
|---------|------|
| `X-Cache` | `HIT` または `MISS` |
| `ETag` | コンテンツの SHA256 ハッシュ |

## 開発

```bash
# フォーマット
make fmt

# Lint
make lint

# テスト
make test
```
