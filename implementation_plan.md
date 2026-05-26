# fits-view 実装計画書

> コンセプト仕様書 `fitsview_concept_spec.md` に基づく段階的実装計画
> 作成日：2026-05-26

---

## 全体方針

- 各 Step は独立してビルド・動作確認できる状態で完了する
- Step 1 → Step 5 の順に依存関係が積み上がる（前 Step が完了してから次へ）
- 各 Step の末尾に「完了基準」を設け、曖昧な完了を防ぐ
- WSL2 + GPU なし環境でも全 Step が動作すること（CPUフォールバック優先）

---

## クレート構成（全 Step 共通）

```
fits-viewer/
├── Cargo.toml               # ワークスペースルート
├── crates/
│   ├── fitsview-core/       # FITS I/O・レンダリング・タイル管理
│   ├── fitsview-gui/        # egui GUIアプリ
│   └── fitsview-cli/        # CLIエントリポイント（clap）
├── assets/
│   └── colormaps/           # LUTデータ（PNG or バイナリ）
└── tests/
    └── fixtures/            # テスト用FITSファイル（小サイズ）
```

---

## Step 0 — プロジェクト基盤整備

**目的**：Cargo ワークスペース・依存関係・CI の骨格を作る。コードは最小限。

### タスク一覧

#### 0-1. Cargo ワークスペース初期化

- ルート `Cargo.toml` でワークスペースを定義
- 3 クレートを空の状態で作成：`fitsview-core`、`fitsview-gui`、`fitsview-cli`
- `fitsview-cli` が `fitsview-gui` と `fitsview-core` に依存する構成にする

#### 0-2. 依存クレートの追加

| クレート | バージョン | 追加先 |
|---|---|---|
| `fitsrs` | latest | fitsview-core |
| `memmap2` | latest | fitsview-core |
| `rayon` | latest | fitsview-core |
| `tokio` | 1.x (full) | fitsview-core |
| `serde` + `serde_json` | latest | fitsview-core |
| `egui` + `eframe` | 0.29.x | fitsview-gui |
| `wgpu` | latest | fitsview-gui (feature = "gpu") |
| `clap` | 4.x | fitsview-cli |
| `notify` | latest | fitsview-gui |
| `lru` | latest | fitsview-core |
| `anyhow` | latest | 全クレート |
| `log` + `env_logger` | latest | 全クレート |

#### 0-3. Feature フラグ定義

```toml
# fitsview-gui/Cargo.toml
[features]
default = ["gpu"]
gpu = ["wgpu"]
cpu-only = []
```

#### 0-4. GitHub Actions CI 設定

- `cargo check`・`cargo test`・`cargo clippy` を Linux / macOS / Windows で実行
- `--no-default-features --features cpu-only` でも CI が通ること

#### 0-5. テスト用 FITS ファイルの配置

- パブリックドメインの小サイズ FITS（< 1MB）を `tests/fixtures/` に配置
- NASA/STScI の公開データや fitsrs のテストデータを流用

### 完了基準

- `cargo build --workspace` がエラーなく通る
- `cargo test --workspace` が通る（テストは空でも可）
- WSL2 CPU-only ビルド：`cargo build --no-default-features --features cpu-only` が通る

---

## Step 1 — MVP：最小限の動くビューア

**目的**：FITSファイルを読み込んでウィンドウに画像を表示する、最もシンプルな動作を実現する。

### タスク一覧

#### 1-1. `fitsview-core`: FITS 読み込みモジュール

ファイル：`crates/fitsview-core/src/fits_reader.rs`

- `fitsrs` を使い、ファイルパスからプライマリ HDU のイメージデータを読む
- 対応 BITPIX：±8, ±16, ±32, ±64（全整数型・浮動小数点型）
- 読み込んだデータを `Vec<f32>` に正規化して返す
- FITS ヘッダキーワードを `HashMap<String, String>` として返す

```rust
// 公開API の概形
pub struct FitsImage {
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub header: HashMap<String, String>,
    pub bitpix: i32,
}

pub fn load_fits(path: &Path) -> anyhow::Result<FitsImage>
```

#### 1-2. `fitsview-core`: zscale スケーリング

ファイル：`crates/fitsview-core/src/scale.rs`

- `zscale` アルゴリズムの実装（DS9 互換、サンプリング + 線形フィット）
- `linear`・`log`・`sqrt`・`asinh`・`minmax` も実装
- 入力：`&[f32]`、出力：`ScaleResult { vmin: f32, vmax: f32 }`

```rust
pub enum ScaleMode { ZScale, Linear, Log, Sqrt, Asinh, MinMax }

pub struct ScaleResult { pub vmin: f32, pub vmax: f32 }

pub fn compute_scale(data: &[f32], mode: ScaleMode) -> ScaleResult
```

#### 1-3. `fitsview-core`: カラーマップ適用

ファイル：`crates/fitsview-core/src/colormap.rs`

- グレースケール・viridis・plasma・inferno・hot・rainbow を実装
- 入力：正規化済み `f32`（0.0〜1.0）→ 出力：`[u8; 4]`（RGBA）
- LUT は `const` 配列としてコンパイル時に埋め込む（外部ファイル不要）

```rust
pub enum Colormap { Gray, Viridis, Plasma, Inferno, Hot, Rainbow }

pub fn apply_colormap(value: f32, cmap: Colormap) -> [u8; 4]
pub fn render_to_rgba(data: &[f32], vmin: f32, vmax: f32, cmap: Colormap) -> Vec<u8>
```

#### 1-4. `fitsview-gui`: メインウィンドウ（egui）

ファイル：`crates/fitsview-gui/src/app.rs`

- `eframe::App` を実装した `FitsViewApp` 構造体
- 起動引数でファイルパスを受け取り、`load_fits` で読み込む
- `egui::Image`（または `egui::Painter`）でRGBA画像をテクスチャとして表示
- ウィンドウタイトル：`fits-view — <ファイル名>`

#### 1-5. `fitsview-gui`: パン・ズーム操作

ファイル：`crates/fitsview-gui/src/viewport.rs`

- マウスドラッグでパン
- マウスホイール / `Ctrl++` / `Ctrl+-` でズーム
- `Ctrl+0` でフィットトゥウィンドウ
- `ViewState { offset: Vec2, zoom: f32 }` 構造体で状態管理

#### 1-6. `fitsview-gui`: ヘッダビューア（右パネル）

ファイル：`crates/fitsview-gui/src/header_panel.rs`

- FITSヘッダキーワードをスクロール可能な右パネルに表示
- `Ctrl+H` でパネルの表示・非表示切り替え
- キーワード検索フィールドを上部に設置

#### 1-7. `fitsview-cli`: エントリポイント

ファイル：`crates/fitsview-cli/src/main.rs`

- `clap` でコマンドライン引数をパース（この Step ではファイルパスのみ）
- `fits-view <file.fits>` でGUIを起動

### ファイル構成（Step 1 完了後）

```
crates/
├── fitsview-core/src/
│   ├── lib.rs
│   ├── fits_reader.rs
│   ├── scale.rs
│   └── colormap.rs
├── fitsview-gui/src/
│   ├── lib.rs
│   ├── main.rs       # eframe::run_native
│   ├── app.rs
│   ├── viewport.rs
│   └── header_panel.rs
└── fitsview-cli/src/
    └── main.rs
```

### 完了基準

- `fits-view tests/fixtures/sample.fits` でウィンドウが開き、画像が表示される
- パン・ズームが動作する
- ヘッダパネルにキーワードが表示される
- `Ctrl+H` でヘッダパネルが切り替わる
- CPU-only ビルドで動作する

---

## Step 2 — VSCode ライク UI

**目的**：ファイルエクスプローラー・マルチタブ・分割表示を実装し、ディレクトリ単位の作業フローを実現する。

### タスク一覧

#### 2-1. ファイルエクスプローラー（左サイドバー）

ファイル：`crates/fitsview-gui/src/file_explorer.rs`

- ディレクトリツリーを再帰表示
- `.fits` / `.fit` / `.fits.gz` のみアイコン付きでハイライト
- ファイルクリック → タブで開く
- 右クリックコンテキストメニュー：「開く」「情報を表示」「サムネイル生成」
- `Ctrl+B` でサイドバー表示・非表示切り替え

#### 2-2. `notify` によるディレクトリウォッチ

ファイル：`crates/fitsview-gui/src/dir_watcher.rs`

- バックグラウンドスレッドで `notify::Watcher` を起動
- 新ファイル追加・削除を検知して `mpsc::channel` でGUIスレッドへ通知
- GUIスレッドはイベントを受け取り、エクスプローラーの表示を更新

#### 2-3. タブマネージャー

ファイル：`crates/fitsview-gui/src/tab_manager.rs`

- `Vec<Tab>` で開いているファイルを管理
- `Tab { id, path, fits_image, view_state, display_config }`
- タブバーの描画（egui でカスタム実装）
- `Ctrl+Tab` / `Ctrl+Shift+Tab` で切り替え
- `Ctrl+W` でアクティブタブを閉じる
- タブのドラッグ&ドロップで並び替え（egui DragAndDrop）

#### 2-4. 分割表示・比較モード

ファイル：`crates/fitsview-gui/src/split_view.rs`

- `SplitLayout { Single, SideBySide, Grid2x2 }` 列挙
- `Ctrl+\` で縦分割、`Ctrl+K Ctrl+\` で横分割
- 各ペインに独立した `ViewState` を持つ
- リンクモード ON 時：パン・ズームを全ペインに同期
- カーソル座標を全ペインに同期表示

#### 2-5. ステータスバー

ファイル：`crates/fitsview-gui/src/status_bar.rs`

- 下部に固定表示
- 左：ファイル名・HDU番号
- 中央：カーソル下のピクセル座標と値
- 右：現在のスケール・カラーマップ・ズーム倍率

#### 2-6. HDU ナビゲーション

- `fitsrs` の HDU 一覧取得機能を使い、全 HDU をサイドパネルに表示
- `[` / `]` キーで前後の HDU に切り替え
- HDU のタイプ（IMAGE / TABLE / COMPRESSED_IMAGE）をアイコンで区別

#### 2-7. コマンドパレット（基本版）

ファイル：`crates/fitsview-gui/src/command_palette.rs`

- `Ctrl+Shift+P` でオーバーレイ表示
- コマンド一覧をインクリメンタル検索
- 実装するコマンド：
  - `Open File...`
  - `Open Directory...`
  - `Set Colormap: <name>`
  - `Set Scale: <mode>`
  - `Toggle Sidebar`
  - `Toggle Header Panel`
  - `Split View Vertical`
  - `Split View Horizontal`

#### 2-7b. イベントリスト（BINTABLE EVENTS）→ 画像変換

ファイル：`crates/fitsview-core/src/event_image.rs`

**背景**：X 線天文衛星（Suzaku, Chandra, XMM-Newton など）の観測データは、イメージ HDU を持たず `BINTABLE` の `EVENTS` 拡張に光子 1 個ずつの `(X, Y, TIME, ENERGY, ...)` を格納する。これをビニングして 2D ヒストグラムを作り、画像として表示する。

**空間列の自動検出**：

| 優先順位 | 列名候補 | 用途 |
|---|---|---|
| 1 | `X`, `Y` | 検出器座標（Suzaku, Chandra） |
| 2 | `DETX`, `DETY` | 検出器座標（XMM-Newton） |
| 3 | `RA`, `DEC` | 天球座標（WCS ベース） |
| 4 | `RAWX`, `RAWY` | 生 CCD 座標 |

**実装内容**：

```rust
pub struct EventImage {
    pub data: Vec<f32>,      // ビニング済みカウントマップ
    pub width: usize,
    pub height: usize,
    pub bin_size: f64,        // ピクセル/ビン（デフォルト 1.0）
    pub x_col: String,        // 使用した X 列名
    pub y_col: String,        // 使用した Y 列名
    pub total_events: usize,
    pub header: HashMap<String, String>,
}

pub fn bin_events(path: &Path, hdu_index: usize, bin_size: f64) -> anyhow::Result<EventImage>
```

処理フロー：
1. BINTABLE HDU のカラム一覧をスキャンし、空間列を優先順位で自動選択
2. `(X_min, X_max, Y_min, Y_max)` を求めて出力画像サイズを決定（最大 4096×4096 に制限）
3. `rayon::par_iter` で各イベントを 2D ヒストグラムにビニング
4. `FitsImage` と同等の `Vec<f32>` カウントマップとして返す

エネルギーフィルタリング（将来拡張のための API 設計のみ Step 2 で定義）：
```rust
pub struct EventFilter {
    pub energy_range: Option<(f32, f32)>,  // keV
    pub time_range: Option<(f64, f64)>,    // s
}
```

**fitsview-gui 側対応**：

- `app.rs` の `load_fits` の前段で HDU タイプを判定するディスパッチャーを追加
- EVENTS BINTABLE を検出したら `bin_events` を呼び出し、結果を `FitsImage` として扱う
- ステータスバーにイベントモード表示：`EVENTS | X/Y | 281万 events | bin=1.0`
- ビンサイズ変更 UI：ステータスバーの `bin=` 表示をクリックで数値入力

#### 2-8. `fits-view .` の実装

- CLI で `.` または ディレクトリパスを受け取った場合、エクスプローラーをそのパスで開く
- `fits-view a.fits b.fits c.fits` で複数ファイルをタブで開く

#### 2-9. キーボードショートカット一式

仕様書 §4.1 の全ショートカットを実装：

| 操作 | ショートカット |
|---|---|
| ファイルを開く | `Ctrl+O` |
| ディレクトリを開く | `Ctrl+K Ctrl+O` |
| タブを閉じる | `Ctrl+W` |
| タブ切り替え | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| 分割表示（縦） | `Ctrl+\` |
| 分割表示（横） | `Ctrl+K Ctrl+\` |
| サイドバー表示切替 | `Ctrl+B` |
| ヘッダビューア表示切替 | `Ctrl+H` |
| ズームイン/アウト | `Ctrl++` / `Ctrl+-` |
| フィットトゥウィンドウ | `Ctrl+0` |
| HDU切替（前/次） | `[` / `]` |
| コマンドパレット | `Ctrl+Shift+P` |

### ファイル構成（Step 2 追加分）

```
crates/fitsview-gui/src/
├── file_explorer.rs     # NEW
├── dir_watcher.rs       # NEW
├── tab_manager.rs       # NEW
├── split_view.rs        # NEW
├── status_bar.rs        # NEW
├── command_palette.rs   # NEW
└── app.rs               # 既存：レイアウト全体を統合
```

### 完了基準

- `fits-view .` でカレントディレクトリのエクスプローラーが開く
- FITSファイルをクリックするとタブで開く
- 3ファイル以上のタブを同時に開いて切り替えられる
- 縦・横分割で2ファイルを並べて表示できる
- コマンドパレットでカラーマップを変更できる
- 新ファイルをディレクトリに追加するとエクスプローラーが自動更新される
- **`fits-view tests/fixtures/tycho.fits` でイベントリストがカウントマップ画像として表示される**
- **イベントファイルでステータスバーに `EVENTS | X/Y | N events | bin=1.0` が表示される**

---

## Step 3 — 巨大データ対応

**目的**：GB〜TB 規模の FITS ファイルを RAM に載せずにタイルストリーミングで表示する。GPU 描画レイヤーを追加する。

### タスク一覧

#### 3-1. `memmap2` によるメモリマップI/O

ファイル：`crates/fitsview-core/src/mmap_reader.rs`

- `fitsrs` の HDU オフセットを取得し、`memmap2::Mmap` でイメージデータ領域をマッピング
- ファイル全体を読み込まず、タイル要求時にオフセット計算でアクセス
- FITS の BITPIX に応じたバイト列 → `f32` 変換をゼロコピーで実装

```rust
pub struct MmapFitsImage {
    mmap: Mmap,
    data_offset: u64,   // ヘッダ後のデータ開始位置
    pub width: usize,
    pub height: usize,
    pub bitpix: i32,
}

impl MmapFitsImage {
    pub fn read_tile(&self, tx: usize, ty: usize, tile_size: usize) -> Vec<f32>
}
```

#### 3-2. タイルマネージャー（LRU キャッシュ）

ファイル：`crates/fitsview-core/src/tile_manager.rs`

- タイルサイズ：デフォルト 512×512 px（設定変更可能）
- LRU キャッシュ上限：設定値（デフォルト 512MB 相当）
- タイルキー：`(file_id, hdu_index, zoom_level, tx, ty)`
- キャッシュミス時は `MmapFitsImage::read_tile` を呼び出す

```rust
pub struct TileKey {
    pub file_id: u64,
    pub hdu: usize,
    pub zoom: u8,
    pub tx: usize,
    pub ty: usize,
}

pub struct TileManager {
    cache: LruCache<TileKey, Arc<TileData>>,
    max_memory_bytes: usize,
}

impl TileManager {
    pub fn get_or_load(&mut self, key: TileKey, source: &MmapFitsImage) -> Arc<TileData>
}
```

#### 3-3. 非同期タイルロード（tokio + rayon）

ファイル：`crates/fitsview-core/src/tile_loader.rs`

- `tokio::spawn` でタイルロードをバックグラウンド非同期タスクに
- CPU バウンドな BITPIX 変換は `rayon::spawn` でスレッドプールに投入
- GUI スレッドはプレースホルダー（灰色タイル）を即座に表示し、ロード完了後に差し替え

```rust
pub struct TileLoader {
    tx: mpsc::Sender<TileResult>,
    rx: mpsc::Receiver<TileResult>,
}

impl TileLoader {
    pub fn request(&self, key: TileKey, source: Arc<MmapFitsImage>)
    pub fn poll_completed(&mut self) -> Vec<TileResult>
}
```

#### 3-4. 進捗インジケーター

ファイル：`crates/fitsview-gui/src/progress.rs`

- ステータスバーにローディングスピナーと「ロード中 N/M タイル」を表示
- 全タイル完了後は自動的に非表示

#### 3-5. GPU レンダリングレイヤー（wgpu）

ファイル：`crates/fitsview-gui/src/renderer/gpu.rs`

**初期化処理：**
1. `wgpu::Instance::create()` でデバイス取得を試みる
2. 失敗 / GPU OOM 発生時は `cpu.rs` にフォールバック
3. `--renderer cpu` フラグ指定時は強制 CPU

**GPU パイプライン：**
1. タイルデータ（`f32` テクスチャ）を GPU バッファに転送
2. Compute Shader でトーンマッピング + カラーマップ（1D LUT テクスチャ）適用
3. Fragment Shader で画面に描画
4. egui の `egui_wgpu` バックエンドと統合

```
crates/fitsview-gui/src/renderer/
├── mod.rs         # Renderer trait + 自動選択ロジック
├── gpu.rs         # wgpu 実装
├── cpu.rs         # rayon + RGBA バッファ実装（Step 1 の実装を移動）
└── shaders/
    ├── tonemap.wgsl
    └── colormap.wgsl
```

#### 3-6. ズームレベルの LOD（Level of Detail）

- ズームアウト時はより荒いズームレベルのタイルを使用（ミップマップ相当）
- ズームレベルは `0`（最粗）〜 `N`（元解像度）
- 低ズームレベルタイルはファイル開封時に非同期プリフェッチ

#### 3-7. 圧縮 FITS 対応

- `fits_reader.rs` を拡張し、`ZTILE_COMPRESS` HDU を検出
- `rayon::par_iter` で並列展開
- RICE、GZIP、PLIO 圧縮をサポート（fitsrs の機能を活用）

#### 3-8. 大ファイル対応 UI

- ファイルエクスプローラーで大ファイル（> 1GB）にはサイズバッジを表示
- タブタイトルにロード進捗（%）を表示
- メモリ使用量をステータスバーに表示

### 完了基準

- 10GB の FITS ファイルを開き、最初のタイルが 2 秒以内に表示される
- パン・ズーム中に RAM が増え続けない（LRU キャッシュが機能している）
- WSL2 / GPU なし環境で CPU フォールバックが自動的に動作する
- GPU 環境で 60fps のパン・ズームが動作する

---

## Step 4 — CLI モード

**目的**：ヘッドレスコマンドとして `fits-view render`・`fits-view info`・`fits-view stats` を実装し、パイプライン・CI/CD での利用を可能にする。

### タスク一覧

#### 4-1. CLI サブコマンド設計（clap）

ファイル：`crates/fitsview-cli/src/main.rs`

```rust
// clap のコマンド構造
fits-view [PATH]                          // GUI モード（デフォルト）
fits-view render <PATH> [OPTIONS]         // ヘッドレス描画
fits-view info <PATH> [OPTIONS]           // ヘッダ情報出力
fits-view stats <PATH> [OPTIONS]          // 統計情報出力
fits-view check <PATH> [OPTIONS]          // アサーションチェック
```

#### 4-2. `render` サブコマンド

ファイル：`crates/fitsview-cli/src/commands/render.rs`

オプション：

| オプション | デフォルト | 説明 |
|---|---|---|
| `--output <path>` | 必須 | 出力ファイルパス |
| `--output-dir <dir>` | — | バッチ出力先ディレクトリ |
| `--format <fmt>` | `png` | png / jpeg / pdf / svg |
| `--size <WxH>` | `512x512` | 出力解像度 |
| `--colormap <name>` | `gray` | カラーマップ名 |
| `--scale <mode>` | `zscale` | スケーリングモード |
| `--vmin <val>` | — | 手動最小値 |
| `--vmax <val>` | — | 手動最大値 |
| `--hdu <n>` | `1` | HDU 番号 |
| `--renderer <mode>` | `auto` | `auto` / `cpu` / `gpu` |

実装：
- GUI なしで `fitsview-core` の描画パイプラインをそのまま呼び出す
- PNG / JPEG 出力：`image` クレートを使用
- PDF / SVG 出力：`printpdf` / `svg` クレートを使用
- ディレクトリ指定時は `rayon::par_iter` で並列処理

#### 4-3. `info` サブコマンド

ファイル：`crates/fitsview-cli/src/commands/info.rs`

オプション：

| オプション | 説明 |
|---|---|
| `--format <fmt>` | `text`（デフォルト）/ `json` / `csv` |
| `--hdu <n>` | 特定 HDU のみ表示 |

出力：
- `text`：ヘッダキーワードを整形テキストで表示
- `json`：全 HDU のヘッダを JSON 配列で出力
- `csv`：`keyword,value,comment` 形式

```bash
fits-view info image.fits --format json
# → {"hdu": [{"index": 0, "type": "IMAGE", "keywords": {...}}, ...]}
```

#### 4-4. `stats` サブコマンド

ファイル：`crates/fitsview-cli/src/commands/stats.rs`

オプション：

| オプション | 説明 |
|---|---|
| `--hdu <n>` | 対象 HDU |
| `--region <x1:x2,y1:y2>` | 計算領域（省略時は全体） |
| `--format <fmt>` | `text` / `json` / `csv` |

出力統計値：min, max, mean, median, std, sum, npix（NaN除外）

#### 4-5. `check` サブコマンド

ファイル：`crates/fitsview-cli/src/commands/check.rs`

```bash
fits-view check image.fits --assert "NAXIS == 2" --assert "BITPIX == -32"
# 成功時：exit code 0
# 失敗時：exit code 1、エラーメッセージを stderr に出力
```

- アサーション式のパーサーを実装（簡易式評価：`==`, `!=`, `<`, `>`, `<=`, `>=`）
- CI スクリプトでの品質チェックに使用

#### 4-6. GUI モードとの統一（単一バイナリ）

- `fitsview-cli` が PATH / サブコマンドの有無でモードを自動判定
- `fits-view .` → GUI 起動
- `fits-view render ...` → ヘッドレス
- `--headless` フラグで GUI を強制無効化

#### 4-7. バッチ処理最適化

```bash
# xargs 並列対応
ls *.fits | xargs -P 8 fits-view render --output-dir ./thumbs/ --size 256x256

# fits-view 自身の --jobs オプション（より効率的）
fits-view render /data/obs/ --output-dir ./thumbs/ --jobs 8
```

- `--jobs N` で内部 rayon スレッドプールサイズを設定

### 完了基準

- `fits-view render sample.fits --output thumb.png --size 256x256` で PNG が生成される
- `fits-view info sample.fits --format json` が有効な JSON を stdout に出力する
- `fits-view check sample.fits --assert "NAXIS == 2"` が正しい終了コードを返す
- `ls *.fits | xargs -P 4 fits-view render --output-dir ./thumbs/` が動作する
- `fits-view stats sample.fits --format csv | python -c "import sys; print(sys.stdin.read())"` が動作する

---

## Step 5 — 統合・拡張

**目的**：セッション管理・WCS・PDF書き出し・Wasm・Python バインディングを実装し、プロダクト品質に仕上げる。

### タスク一覧

#### 5-1. セッション管理（`.fvs` 形式）

ファイル：`crates/fitsview-core/src/session.rs`

```rust
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub version: String,
    pub workspace: PathBuf,
    pub files: Vec<FileState>,
    pub layout: LayoutKind,
}

#[derive(Serialize, Deserialize)]
pub struct FileState {
    pub path: PathBuf,
    pub hdu: usize,
    pub display: DisplayConfig,
    pub annotations: Vec<Annotation>,
}
```

- `fits-view --session obs.fvs` でセッション復元
- GUI の「File → Save Session」「File → Open Session」メニュー
- 終了時に「最後のセッション」を自動保存（`~/.config/fits-view/last_session.fvs`）
- `.fvs` ファイルは JSON で人間が読みやすい形式

#### 5-2. WCS（World Coordinate System）対応

ファイル：`crates/fitsview-core/src/wcs.rs`

- FITS ヘッダの WCS キーワード（`CRPIX`, `CRVAL`, `CDELT`, `CD_i_j`）をパース
- ピクセル座標 → 赤経・赤緯変換（線形・Gnomonic 射影）
- ステータスバーのカーソル座標を WCS 座標でリアルタイム表示
- 赤経・赤緯グリッドのオーバーレイ描画

```rust
pub struct WcsTransform { /* WCS パラメータ */ }

impl WcsTransform {
    pub fn from_header(header: &HashMap<String, String>) -> Option<Self>
    pub fn pixel_to_world(&self, x: f64, y: f64) -> (f64, f64)  // (ra, dec) in degrees
    pub fn world_to_pixel(&self, ra: f64, dec: f64) -> (f64, f64)
}
```

#### 5-3. アノテーション機能

ファイル：`crates/fitsview-gui/src/annotation.rs`

- GUI でサークル・矩形・テキストラベルを追加
- セッションファイルに保存・復元
- WCS 座標でアノテーション位置を記録（ファイル間で整合）

#### 5-4. 論文品質の書き出し

- `fits-view render` に `--format pdf` / `--format svg` を追加
- 余白・軸ラベル・カラーバー・タイトルの設定オプション
- DPI 指定（デフォルト 150dpi、論文用 300dpi）

#### 5-5. Wasm 対応（ブラウザ版）

ファイル：`crates/fitsview-wasm/`（新規クレート）

- `trunk` + `wasm-bindgen` でブラウザ向けビルド
- `eframe` の `web_app` feature を活用（egui は Wasm 対応済み）
- `memmap2` は Wasm 非対応のため `XHR Range リクエスト`で代替タイル取得
- GPU：WebGPU（Chrome 113+）または WebGL フォールバック
- 配布：単一 HTML + WASM ファイル（S3 や GitHub Pages にデプロイ可能）

```bash
# Wasm ビルド
cd crates/fitsview-wasm && trunk build --release

# ローカル確認
trunk serve
```

#### 5-6. Python バインディング（PyO3）

ファイル：`crates/fitsview-py/`（新規クレート）

```python
import fitsview

# ファイルを開く
img = fitsview.open("image.fits")

# ヘッダ取得
header = img.header()  # dict

# ピクセルデータ取得（numpy 配列として）
data = img.data(hdu=1)  # np.ndarray[float32]

# GUI でインタラクティブ表示
viewer = fitsview.show(img, colormap="viridis", scale="zscale")

# GUI でクリックした座標を取得
coords = viewer.pick()  # (x, y, value)
```

- `maturin` でビルド・`PyPI` に公開
- `numpy` との相互運用（`PyArray` 経由）

#### 5-7. パフォーマンスプロファイリングと最適化

- `criterion` ベンチマークを各クレートに追加
- タイルロード・レンダリング・WCS変換のベンチマーク
- `perf` / `flamegraph` によるホットパス特定
- KPI（§9）の全指標を達成するまで最適化

#### 5-8. パッケージング・配布

- GitHub Actions で Linux / macOS / Windows のリリースバイナリを生成
- `cargo-dist` でリリース自動化
- Homebrew formula 作成（macOS）
- `cargo install fits-view` の動作確認

### 完了基準

- セッション保存・復元が動作する（GUI 再起動後も同じ状態が復元される）
- WCS 対応ファイルでカーソル下の RA/Dec がステータスバーに表示される
- `trunk build` で Wasm バイナリが生成され、Chrome でローカル動作する
- `pip install fitsview-py` で Python バインディングがインストールできる
- KPI の全指標（起動 < 1 秒、10GB < 2 秒表示、GPU 60fps、メモリ < 2GB）を達成

---

## 依存関係サマリー

```
Step 0 → Step 1 → Step 2 → Step 3 → Step 4 → Step 5
 基盤      MVP    VSCode UI  巨大ファイル   CLI    統合拡張
```

- **Step 3** の `tile_manager.rs`・`mmap_reader.rs` は Step 1 の `fits_reader.rs` を置き換える（非破壊的：大ファイル用パスを追加）
- **Step 4** の CLI は Step 3 の描画パイプラインをそのまま呼び出す（GUI なし）
- **Step 5** の Wasm / Python は `fitsview-core` を再利用する（新クレートとして追加）

---

## リスクと対策

| リスク | 対策 |
|---|---|
| `fitsrs` が一部 FITS 方言に非対応 | 問題発生時は `fitsrs` に PR / フォーク、または独自パーサー部分実装 |
| イベントリストの列名が衛星ごとに異なる | 優先順位付きの列名候補リストで対応。未検出時はユーザーが列を手動選択できる UI を追加 |
| 大量イベント（1億件以上）のビニングが遅い | `rayon::par_iter` で並列化。Step 3 の `memmap2` と統合してゼロコピー読み込みに移行 |
| WSL2 で wgpu が動作しない | Step 1〜2 は CPU-only で開発。Step 3 で GPU 追加時に WSL2 `/dev/dxg` 経由を検証 |
| `memmap2` の Windows/WSL2 挙動差異 | `/mnt/c/` は回避し WSL2 ネイティブ FS で開発・テスト |
| egui のカスタム UI 実装コスト | タブバー・ファイルエクスプローラーは egui の Painter API で自作。工数が大きい場合は既存 Widget を優先 |
| Wasm の FITS ファイルアクセス | Range リクエスト + HTTP サーバーが必要。ローカルファイルは File API 経由で対応 |

---

## 推奨開発順序

各 Step 内の作業順：

1. `fitsview-core` のデータ構造・ロジックを先に実装してユニットテスト
2. `fitsview-gui` の UI コンポーネントを追加して動作確認
3. `fitsview-cli` のコマンドを最後に統合

これにより GUI なし環境（CI）でもコアロジックのテストが常に動作する状態を維持できる。

---

*最終更新：2026-05-26 | ステータス：計画段階*
