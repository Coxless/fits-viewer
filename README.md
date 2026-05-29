# fits-view

高速・軽量な FITS ファイルビューア。Rust 製、GPU レンダリング対応。

天文学者とデータパイプラインエンジニアが GB〜TB 規模の FITS ファイルを快適に扱えるよう設計されています。

---

## インストール / ビルド

Rust ツールチェーン（MSRV 1.75）が必要です。

```bash
# GPU 有効（デフォルト）
cargo build --release

# WSL2 / SSH / CI など GPU が使えない環境
cargo build --release --no-default-features --features cpu-only
```

ビルド後のバイナリは `target/release/fits-view` に生成されます。

---

## 起動方法

| コマンド | 動作 |
|---|---|
| `fits-view` | 空の GUI を起動 |
| `fits-view image.fits` | ファイルを開いて GUI 起動 |
| `fits-view dir/` | ディレクトリを開いて GUI 起動 |
| `fits-view a.fits b.fits` | 複数ファイルをタブで開く |
| `fits-view <サブコマンド>` | ヘッドレスモード（GUI なし） |

---

## GUI の使い方

### 画面レイアウト

```
┌─────────────────────────────────────────────────────┐
│  タブバー（開いているファイル一覧）                     │
├──────────┬──────────────────────────┬───────────────┤
│          │                          │               │
│ファイル  │      ビューポート         │  ヘッダー     │
│エクスプ  │    （画像表示エリア）      │  パネル       │
│ローラー  │                          │               │
│          │                          │               │
├──────────┴──────────────────────────┴───────────────┤
│  ステータスバー（ファイル名 · ピクセル値 · WCS座標 …） │
└─────────────────────────────────────────────────────┘
```

### ファイルを開く

- ファイルエクスプローラー（左サイドバー）でファイルをクリック
- コマンドパレット `Ctrl+Shift+P` → "Open File..."
- 起動時の引数でパスを渡す

### ナビゲーション

| 操作 | 動作 |
|---|---|
| スクロール | ズームイン / アウト（カーソル位置中心） |
| 左ドラッグ | パン（画像を移動） |
| 右ドラッグ | コントラスト（X方向）・バイアス（Y方向）調整 |
| `Ctrl+0` | 画像をウィンドウに合わせる（Fit to Window） |
| `Ctrl++` | ズームイン |
| `Ctrl+-` | ズームアウト |

### キーボードショートカット一覧

#### パネル表示

| キー | 動作 |
|---|---|
| `Ctrl+B` | ファイルエクスプローラー 表示/非表示 |
| `Ctrl+H` | ヘッダーパネル 表示/非表示 |
| `Ctrl+I` | 統計パネル 表示/非表示 |
| `Ctrl+Shift+P` | コマンドパレット を開く |

#### タブ操作

| キー | 動作 |
|---|---|
| `Ctrl+Tab` | 次のタブへ |
| `Ctrl+Shift+Tab` | 前のタブへ |
| `Ctrl+W` | 現在のタブを閉じる |

#### ビュー分割

| キー | 動作 |
|---|---|
| `Ctrl+\` | 左右 2 分割（SideBySide）に切り替え |
| `Ctrl+K` → `Ctrl+\` | 2×2 グリッド分割に切り替え |

#### HDU ナビゲーション

| キー | 動作 |
|---|---|
| `[` | 前の HDU へ |
| `]` | 次の HDU へ |

ヘッダーパネルの HDU セレクタからも切り替えられます。

#### その他

| キー | 動作 |
|---|---|
| `Ctrl+X` | クロスヘア 表示/非表示 |
| `Ctrl+L` | ブリンク（複数タブを交互に切り替え） 開始/停止 |
| `Ctrl+R` | DS9 リージョンファイルを読み込む |

---

### コマンドパレット

`Ctrl+Shift+P` でコマンドパレットを開き、コマンド名の一部を入力してフィルタリングできます。

利用可能なコマンド:

| コマンド | 説明 |
|---|---|
| Open File... | ファイルを開く |
| Open Directory... | ディレクトリをサイドバーに設定 |
| Set Colormap: \<名前\> | カラーマップを変更 |
| Set Scale: \<名前\> | スケーリングアルゴリズムを変更 |
| Toggle Sidebar | サイドバー 表示/非表示 |
| Toggle Header Panel | ヘッダーパネル 表示/非表示 |
| Split View: Vertical | 左右 2 分割 |
| Split View: Horizontal | 2×2 グリッド分割 |
| Fit to Window | 画像をウィンドウに合わせる |
| Toggle Statistics Panel | 統計パネル 表示/非表示 |
| Load Region File... | DS9 .reg ファイルを読み込む |
| Toggle Blink | ブリンク 開始/停止 |
| Reset Contrast/Bias | コントラスト・バイアスをリセット |
| Toggle Crosshair | クロスヘア 表示/非表示 |

---

### カラーマップ

| 名前 | 用途 |
|---|---|
| Gray | 汎用グレースケール |
| Viridis | 知覚的均一、科学可視化の標準 |
| Plasma | 高コントラスト |
| Inferno | 暗背景に映える |
| Hot | 輝度の強調 |
| Rainbow | 広いダイナミックレンジの確認 |

---

### スケーリングアルゴリズム

| 名前 | 説明 |
|---|---|
| ZScale | DS9 互換。天文画像に最適なデフォルト |
| Linear | 線形スケール |
| Log | 対数スケール。広いダイナミックレンジに |
| Sqrt | 平方根スケール |
| ASinh | 逆双曲線正弦。低輝度の細部保存 |
| MinMax | 最小〜最大を全範囲にマッピング |
| HistEq | ヒストグラム均一化 |

---

### コントラスト / バイアス

- **右ドラッグ**: X 方向でコントラスト、Y 方向でバイアスをリアルタイム調整
- **スライダー**: ステータスバー下のスライダーで数値を直接指定
  - Contrast: 0.05 〜 5.0（デフォルト 1.0）
  - Bias: 0.0 〜 1.0（デフォルト 0.5）
- **リセット**: コマンドパレット → "Reset Contrast/Bias"

---

### 統計パネル

`Ctrl+I` または コマンドパレット → "Toggle Statistics Panel" で開きます。

表示される統計値: 総ピクセル数、有効ピクセル数、最小値、最大値、平均値、中央値、標準偏差、合計値

---

### DS9 リージョンファイル

`Ctrl+R` または コマンドパレット → "Load Region File..." で DS9 形式の `.reg` ファイルを画像上に重ねて表示できます。WCS 座標と画像座標の両方に対応しています。

---

### ブリンク

`Ctrl+L` で複数タブをコマ送りのように切り替えます。ステータスバーに `BLINK 0.5s` のように現在の切り替え間隔が表示されます。

---

### 大容量ファイルの扱い

512 MB を超えるファイルは自動的にタイル分割読み込みモードに切り替わります。

- メモリに全データを載せずにゼロコピーでストリーミング表示
- ズームレベルに応じた LOD（解像度レベル）を自動選択
- ステータスバーにタイル読み込み数とキャッシュ使用量を表示

---

## CLI の使い方（ヘッドレスモード）

サブコマンドを指定するとGUIなしで実行されます。

### `render` — FITS 画像を PNG/JPEG に書き出す

```bash
fits-view render image.fits --output thumb.png
fits-view render image.fits --output thumb.jpg --size 512x512
fits-view render image.fits --output thumb.png --scale zscale --colormap viridis
fits-view render image.fits --output hdu2.png --hdu 2
```

| オプション | デフォルト | 説明 |
|---|---|---|
| `--output` | （必須） | 出力ファイルパス（.png / .jpg） |
| `--size WxH` | `256x256` | 出力サイズ |
| `--scale` | `zscale` | スケーリング（zscale / linear / log / sqrt / asinh / minmax / histeq） |
| `--colormap` | `gray` | カラーマップ（gray / viridis / plasma / inferno / hot / rainbow） |
| `--hdu` | 0 | HDU インデックス（0 始まり） |

---

### `info` — FITS ヘッダー / HDU 一覧を表示

```bash
# 全 HDU を一覧表示
fits-view info image.fits

# 特定 HDU のヘッダーを表示
fits-view info image.fits --hdu 0

# 出力形式を指定
fits-view info image.fits --format json
fits-view info image.fits --format csv
```

| オプション | デフォルト | 説明 |
|---|---|---|
| `--hdu` | （省略可） | HDU インデックス。省略すると全 HDU を一覧表示 |
| `--format` | `table` | 出力形式（table / json / csv） |

---

### `stats` — 画像統計を計算・表示

```bash
fits-view stats image.fits
fits-view stats image.fits --hdu 1 --format json
```

出力項目: `npix`, `n_finite`, `min`, `max`, `mean`, `median`, `std_dev`, `sum`

| オプション | デフォルト | 説明 |
|---|---|---|
| `--hdu` | 0 | HDU インデックス |
| `--format` | `table` | 出力形式（table / json / csv） |

---

### `check` — FITS ファイルの検証（CI/CD 向け）

条件を満たせば exit code 0、失敗すれば exit code 1 で終了します。

```bash
# 画像サイズを検証
fits-view check image.fits --min-width 512 --min-height 512

# WCS キーワードの存在を確認
fits-view check image.fits --has-wcs

# 任意のキーワードを確認
fits-view check image.fits --has-keyword EXPTIME --has-keyword FILTER

# 複数条件を組み合わせ
fits-view check image.fits --min-width 1024 --has-wcs --has-keyword OBJECT
```

| オプション | 説明 |
|---|---|
| `--min-width N` | 画像幅が N ピクセル以上であることを要求 |
| `--min-height N` | 画像高さが N ピクセル以上であることを要求 |
| `--has-wcs` | 有効な WCS キーワードの存在を要求 |
| `--has-keyword KEY` | 指定キーワードの存在を要求（複数指定可） |

---

## WSL2 での注意事項

- FITS ファイルは `/home/...` など WSL2 ネイティブパスに置いてください。`/mnt/c/...`（Windows ファイルシステム）上のファイルはパフォーマンスが大幅に低下します。
- GPU が利用できない場合は CPU レンダリングに自動フォールバックします。

---

## 対応 FITS データ型

| BITPIX | データ型 |
|---|---|
| 8 | unsigned int 8-bit |
| 16 | int 16-bit |
| 32 | int 32-bit |
| 64 | int 64-bit |
| -32 | float 32-bit |
| -64 | float 64-bit |

すべての型は内部で `f32` に正規化されてから表示されます。BINTABLE HDU はイベントデータとして自動認識し、binned 画像として表示します。
