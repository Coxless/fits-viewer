# fits-view 実装計画書

> コンセプト仕様書 `fitsview_concept_spec.md` に基づく段階的実装計画
> 作成日：2026-05-26 / 最終更新：2026-05-29

---

## 全体方針

- 各 Step は独立してビルド・動作確認できる状態で完了する
- 前 Step が完了してから次へ（依存関係を崩さない）
- 各 Step の末尾に「完了基準」を設け、曖昧な完了を防ぐ
- WSL2 + GPU なし環境でも全 Step が動作すること（CPUフォールバック優先）
- `cargo clippy -- -D warnings` が常に通ること

---

## クレート構成（全 Step 共通）

```
fits-viewer/
├── Cargo.toml
├── crates/
│   ├── fitsview-core/       # FITS I/O・レンダリング・タイル管理・科学計算
│   ├── fitsview-gui/        # egui GUIアプリ
│   └── fitsview-cli/        # CLIエントリポイント（clap）
├── assets/colormaps/
└── tests/fixtures/
```

---

## フェーズ1：完了済み ✅

Steps 0–4 は実装済み。以下は参照用の記録。

### Step 0 — プロジェクト基盤（完了）

- Cargo ワークスペース、3クレート構成
- 全依存クレートの追加
- `gpu` / `cpu-only` フィーチャーフラグ
- テスト用 FITS ファイル配置

### Step 1 — MVP（完了）

- FITS読み込み（全BITPIX → `Vec<f32>`）
- スケーリング7種（ZScale / Linear / Log / Sqrt / ASinh / MinMax / HistEq）
- カラーマップ6種（Gray / Viridis / Plasma / Inferno / Hot / Rainbow）
- パン・ズーム・フィットトゥウィンドウ
- ヘッダパネル（右パネル、キーワード検索）

### Step 2 — VSCodeライクUI（完了）

- ファイルエクスプローラー（サイドバー、ディレクトリウォッチ）
- マルチタブ管理（`Ctrl+Tab` / `Ctrl+W`）
- 分割表示（Single / SideBySide / Grid2x2）
- ステータスバー（ファイル名・HDU・ピクセル値・WCS座標・ズーム倍率）
- コマンドパレット（`Ctrl+Shift+P`）
- HDU ナビゲーション（`[` / `]`、ヘッダパネル内セレクタ）
- コントラスト/バイアス調整（右ドラッグ + スライダー）
- クロスヘア（`Ctrl+X`）
- ブリンクモード（`Ctrl+L`）
- DS9リージョンファイル表示（`Ctrl+R`）
- BINTABLEイベントリスト → カウントマップ変換
- 統計パネル（`Ctrl+I`）

### Step 3 — 巨大データ対応（完了）

- `memmap2` によるメモリマップI/O
- タイルマネージャー（LRU キャッシュ、512×512px）
- 非同期タイルロード（tokio + rayon）
- LOD（Level of Detail）自動選択
- GPU レンダラー（wgpu）+ CPU フォールバック（rayon）
- 圧縮FITS対応（`compressed_fits.rs`）

### Step 4 — CLIモード（完了）

- `render`：PNG/JPEG ヘッドレスレンダリング
- `info`：ヘッダ / HDU一覧（table / json / csv）
- `stats`：画像統計（table / json / csv）
- `check`：FITS検証（CI/CD用、`--min-width`, `--has-wcs`, `--has-keyword`）

---

## フェーズ2：科学機能コア

> 目標：DS9ユーザーが「これがあれば乗り換えられる」と感じるレベルへ

---

### Step 5 — ヒストグラムパネル + WCSグリッド + 分割連動

**目的**：天文学者が毎日使う基本 UI を完成させる。実装コストに対してリターンが最大のまとまり。

#### 5-1. ヒストグラムパネル

ファイル：`crates/fitsview-gui/src/histogram_panel.rs`

```rust
pub struct HistogramPanel {
    pub visible: bool,
    bins: Vec<u32>,      // ヒストグラムビン
    bin_edges: Vec<f32>, // ビン境界値
    n_bins: usize,       // デフォルト 256
}
```

**UI 仕様：**
- ピクセル値分布をバーチャートで表示（Y軸は対数スケール切替可）
- vmin / vmax のドラッグ可能なカーソルマーカー（赤/青の縦線）
- カーソル位置のビン値をツールチップ表示
- 自動計算ボタン：`ZScale` / `MinMax` / `99%cut` / `99.9%cut`
- 手動入力フィールド：vmin, vmax
- パーセンタイルマーカー（1%, 50%, 99%の位置）

**データ仕様：**
- アクティブタブのデータから非同期でビルド（メインスレッドをブロックしない）
- タイル表示中はビューポート内の可視タイルピクセルからサンプリング（全体の近似）
- vmin / vmax を変更するとタブの `scale_result` と `needs_retexture` を更新

**統合：**
- `app.rs` に `histogram_panel: HistogramPanel` フィールドを追加
- コマンドパレット → "Toggle Histogram" コマンドを追加
- キーボードショートカット：未定（`Ctrl+G` 候補）
- ヘッダパネルと同じ右サイドに配置（タブ切替 or スタック表示）

**完了基準：**
- ヒストグラムが表示され、ドラッグで vmin/vmax が変わり画像が再描画される
- 「ZScale」ボタンで vmin/vmax が自動計算される
- 大容量ファイル（タイルモード）でもハングしない
- 統計パネルに **飽和ピクセル数**（ピクセル値 ≥ `DATAMAX` またはビット深度上限）が表示される（UC-02）

---

#### 5-2. WCSグリッドオーバーレイ

ファイル：`crates/fitsview-core/src/wcs_grid.rs`、`crates/fitsview-gui/src/wcs_overlay.rs`

```rust
// core 側：グリッド線の座標計算
pub struct WcsGridLines {
    pub ra_lines: Vec<Vec<(f64, f64)>>,   // RA格子線（ピクセル座標の折れ線）
    pub dec_lines: Vec<Vec<(f64, f64)>>,  // Dec格子線
    pub labels: Vec<WcsLabel>,
}

pub struct WcsLabel {
    pub text: String,  // "12h34m" or "+25°30'"
    pub x: f64,
    pub y: f64,
}

pub fn compute_wcs_grid(
    wcs: &Wcs,
    width: usize,
    height: usize,
    zoom: f32,
) -> WcsGridLines
```

**仕様：**
- ズームレベルに応じてグリッド間隔を自動調整（1°, 10', 1', 10"...）
- 対応投影：TAN（現在実装済み）+ SIN, ZEA, AIT, CAR, MER（追加）
  - 各投影の `world_to_pixel` を `wcs.rs` に追加実装
- WCS なしファイルではグリッドボタンをグレーアウト
- グリッドの色・透明度・線幅はコマンドパレット / 設定パネルで変更可能
- ラベルはフレーム端（上端・左端）に表示

**統合：**
- タブに `show_wcs_grid: bool` フィールドを追加
- コマンドパレット → "Toggle WCS Grid"（`Ctrl+G` 候補）
- `render_pane` / `render_large_pane` の最後に `draw_wcs_grid(painter, ...)` を呼ぶ

**完了基準：**
- TAN投影ファイルでRA/Decグリッドが正しい位置に描画される
- ズームイン・アウトでグリッド間隔が自動調整される

---

#### 5-3. 分割ビューの連動パン・ズーム（リンクモード）

ファイル：`crates/fitsview-gui/src/split_view.rs`（拡張）

```rust
pub struct SplitView {
    pub layout: SplitLayout,
    pub pane_tab_ids: Vec<Option<u64>>,
    pub active_pane: usize,
    pub linked: bool,  // 追加：リンクモードフラグ
}
```

**仕様：**
- `linked = true` の場合、あるペインでパン・ズームした内容を全ペインに同期
- リンクモードのトグル：コマンドパレット → "Toggle Linked Pan/Zoom"
- WCS ありファイル同士の場合は WCS 座標基準で同期（ピクセル座標ではなく）
- WCS なしの場合はピクセル座標基準で同期
- ステータスバーにリンクモードインジケーター表示（例：`🔗`）

**完了基準：**
- SideBySide で左ペインをズームすると右ペインも同じズームレベルになる

---

#### 5-4. CLIギャップ補完

ファイル：`crates/fitsview-cli/src/main.rs`（拡張）

追加するオプション：

| サブコマンド | オプション | 説明 |
|---|---|---|
| `render` | `--vmin <f32>` | vmin手動指定 |
| `render` | `--vmax <f32>` | vmax手動指定 |
| `render` | `--output-dir <dir>` | バッチ出力先（ディレクトリ全体を処理） |
| `render` | `--jobs <n>` | 並列ジョブ数（デフォルト：CPU数） |
| `stats` | `--region x1:x2,y1:y2` | 計算領域指定 |
| `check` | `--assert <expr>` | 式アサーション（`"NAXIS == 2"` 等） |

`--assert` 式パーサー：
- 対象：FITSヘッダキーワード値と定数の比較
- 演算子：`==`, `!=`, `<`, `>`, `<=`, `>=`
- 文字列値は `FILTER == 'R'` 形式

**完了基準：**
- `fits-view render *.fits --output-dir thumbs/ --jobs 8` が並列動作する
- `fits-view check image.fits --assert "NAXIS == 2" --assert "BITPIX == -32"` が正しい終了コードを返す

---

#### 5-5. ブリンクモードの FPS 制御（UC-03）

ファイル：`crates/fitsview-gui/src/app.rs`（`BlinkState` 拡張）

Step 2 で実装したブリンク機能に速度制御を追加する。

```rust
pub struct BlinkState {
    pub active: bool,
    pub fps: f32,        // デフォルト 2.0、範囲 0.5〜10.0
    last_flip: Instant,
}
```

- ステータスバー右端に FPS スライダー（ブリンク中のみ表示）
- `Ctrl+L` でブリンク ON/OFF、FPS はスライダーで変更
- FPS 設定はセッションに保存・復元

**完了基準：**
- ブリンク FPS を 0.5〜10fps の範囲で変更でき、変更が即座に反映される
- FPS 設定がセッションファイルに含まれ再起動後も保持される

---

### Step 6 — データキューブ（3D/4D FITS）

**目的**：`NAXIS >= 3` のFITSデータを扱えるようにする。分光・IFU・時系列データへの対応。

#### 6-1. `fitsview-core`：キューブ読み込み

ファイル：`crates/fitsview-core/src/cube_reader.rs`

```rust
pub struct FitsCube {
    pub data: Vec<f32>,     // フラット配列 [z][y][x] 順
    pub width: usize,
    pub height: usize,
    pub depth: usize,       // NAXIS3
    pub extra_axes: Vec<usize>, // NAXIS4 以降
    pub header: HashMap<String, String>,
    pub wcs: Option<Wcs>,
    pub spectral_axis: Option<SpectralAxis>,
}

pub struct SpectralAxis {
    pub ctype: String,   // "WAVE", "FREQ", "VRAD" 等
    pub crval: f64,
    pub cdelt: f64,
    pub crpix: f64,
    pub unit: String,
}

impl FitsCube {
    /// z インデックスの 2D スライスを返す（コピー）
    pub fn slice_z(&self, z: usize) -> Vec<f32>
    /// z 範囲を合計したコラプスを返す
    pub fn collapse_z(&self, z_min: usize, z_max: usize) -> Vec<f32>
}
```

- ファイルサイズが `LARGE_FILE_THRESHOLD` を超える場合は `mmap_reader.rs` 経由でスライスごとに読む（RAM に全 cube を載せない）
- `spectral_axis` に `WAV` / `FREQ` / `VRAD` / `TIME` キーワードを対応

#### 6-2. `fitsview-gui`：キューブパネル

ファイル：`crates/fitsview-gui/src/cube_panel.rs`

```rust
pub struct CubePanel {
    pub visible: bool,
    pub z_index: usize,
    pub z_min: usize,
    pub z_max: usize,
    pub playing: bool,
    pub fps: f32,
    pub bounce: bool,
    pub collapse_mode: CollapseMode,  // Single / Sum / Mean
    last_frame: Instant,
}
```

**UI：**
```
┌──────────────────────────────────────────────────────┐
│  Cube  軸: z [WAVE ▼]   depth: 512                  │
│  z = 256 / 511   λ = 656.28 nm                      │
│  [◀] ────────────●────────────── [▶]  [▶▶ Play]     │
│  Step: [1]   FPS: [10.0]   Mode: [Single ▼]         │
│  Collapse range: [0] to [511]  [Apply]               │
└──────────────────────────────────────────────────────┘
```

- スライダー操作でスライスを切り替え
- `Play` で自動スキャン（ループ / バウンス切替）
- スペクトル軸の物理値をリアルタイム表示（波長・周波数・速度・時刻）
- `Collapse` モード：指定z範囲を Sum / Mean して表示
- スペクトル抽出：`Tab`に `spectral_extract_mode: bool` を追加し、クリックした (x,y) のスペクトルをプロットパネルに表示

#### 6-3. `fitsview-gui`：簡易プロットパネル

ファイル：`crates/fitsview-gui/src/plot_panel.rs`

- `egui_plot` を使ったシンプルな折れ線グラフ
- スペクトル抽出結果・輝度プロファイルの共用表示先
- 軸ラベル（単位付き）、ツールチップ（値表示）
- CSVエクスポートボタン

#### 6-4. `app.rs` への統合

- `load_file_full` でNAXIS ≥ 3を検出 → `FileData::Cube(FitsCube)` に分岐
- タブに `cube_panel: Option<CubePanel>` フィールドを追加
- キューブファイルはタブ下部にキューブパネルを自動表示
- コマンドパレット → "Toggle Cube Panel"

**完了基準：**
- `NAXIS == 3` のFITSファイルでキューブパネルが表示される
- スライダーを動かすとスライスが切り替わり再描画される
- Play ボタンでアニメーションが動作する
- クリックした座標のスペクトルがプロットパネルに表示される

---

### Step 7 — セッション管理 + アノテーション

**目的**：IDEとして「再起動したら設定が消える」問題を解決する。

#### 7-1. `fitsview-core`：セッション形式

ファイル：`crates/fitsview-core/src/session.rs`

```rust
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub version: String,
    pub workspace: Option<PathBuf>,
    pub files: Vec<FileState>,
    pub layout: LayoutKind,
    pub linked: bool,
}

#[derive(Serialize, Deserialize)]
pub struct FileState {
    pub path: PathBuf,
    pub hdu: usize,
    pub display: DisplayConfig,
    pub annotations: Vec<Annotation>,
    pub region_files: Vec<PathBuf>,
    pub catalog_files: Vec<PathBuf>,
    pub show_wcs_grid: bool,
    pub show_histogram: bool,
    pub cube_z: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct DisplayConfig {
    pub colormap: String,
    pub scale: String,
    pub vmin: Option<f32>,
    pub vmax: Option<f32>,
    pub zoom: f32,
    pub offset: [f32; 2],
    pub contrast: f32,
    pub bias: f32,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Annotation {
    Circle { ra: f64, dec: f64, radius_arcsec: f64, color: [u8; 4], label: String },
    Box { ra: f64, dec: f64, width_arcsec: f64, height_arcsec: f64, angle_deg: f64, color: [u8; 4], label: String },
    Line { ra1: f64, dec1: f64, ra2: f64, dec2: f64, color: [u8; 4] },
    Text { ra: f64, dec: f64, text: String, color: [u8; 4] },
}

impl Session {
    pub fn save(&self, path: &Path) -> anyhow::Result<()>
    pub fn load(path: &Path) -> anyhow::Result<Self>
    pub fn save_last() -> anyhow::Result<PathBuf>  // ~/.config/fits-view/last_session.fvs
    pub fn load_last() -> anyhow::Result<Self>
}
```

#### 7-2. `fitsview-gui`：アノテーション描画・操作

ファイル：`crates/fitsview-gui/src/annotation.rs`（拡張）

- コマンドパレット → "Annotation Mode" でモード切替
- アノテーションモード中：
  - クリック → テキスト配置
  - ドラッグ → 円・矩形・線を描画
  - WCS ありファイルでは座標を RA/Dec で保存（ファイル間で整合）
- アノテーションをクリックで選択 → ラベル編集・削除
- 色・サイズの設定パネル
- **DS9 `.reg` 形式エクスポート**（UC-17）：コマンドパレット → "Export Regions..." で全アノテーションを `.reg` ファイルに書き出す
  - WCS あり：`fk5` 座標系で出力
  - WCS なし：`image` 座標系で出力

#### 7-3. セッションの保存・復元

- コマンドパレット → "Save Session..." / "Open Session..."
- アプリ終了時に `last_session.fvs` へ自動保存
- 起動時オプション：`fits-view --session obs.fvs`（CLI に `--session <path>` を追加）
- セッションファイルのパスは相対パス優先（ポータビリティのため）

**完了基準：**
- セッションを保存してアプリを再起動し、同じタブ・表示設定・ズームが復元される
- アノテーションが画像上に描画でき、セッションに保存・復元できる
- DS9 `.reg` ファイルとしてエクスポートでき、DS9 で読み込める

---

## フェーズ3：比較・解析機能

> 目標：「DS9でやっていた解析作業のほとんどをfits-viewで完結できる」

---

### Step 8 — RGBコンポジット + コントゥアー + カタログオーバーレイ

**目的**：マルチバンド比較とVO連携を一度に揃える。

#### 8-1. RGBコンポジット

ファイル：`crates/fitsview-core/src/composite.rs`、`crates/fitsview-gui/src/composite_panel.rs`

```rust
pub struct RgbComposite {
    pub r_data: Vec<f32>, pub r_vmin: f32, pub r_vmax: f32, pub r_scale: ScaleMode,
    pub g_data: Vec<f32>, pub g_vmin: f32, pub g_vmax: f32, pub g_scale: ScaleMode,
    pub b_data: Vec<f32>, pub b_vmin: f32, pub b_vmax: f32, pub b_scale: ScaleMode,
    pub width: usize,
    pub height: usize,
}

pub fn render_rgb_to_rgba(composite: &RgbComposite) -> Vec<u8>
```

- コマンドパレット → "RGB Composite..."
- ダイアログで R/G/B に割り当てるタブを選択
- チャンネルごとに Scale / vmin / vmax を設定
- 結果を `FileData::Rgb(RgbComposite)` として新タブで開く
- CLI 対応：`fits-view render r.fits g.fits b.fits --rgb --output composite.png`

#### 8-2. コントゥアーオーバーレイ

ファイル：`crates/fitsview-core/src/contour.rs`、`crates/fitsview-gui/src/contour_overlay.rs`

```rust
pub struct ContourLevel {
    pub value: f32,
    pub color: [u8; 4],
}

pub struct ContourLines {
    pub levels: Vec<ContourLevel>,
    pub polylines: Vec<Vec<(f32, f32)>>,  // ピクセル座標
}

pub fn compute_contours(data: &[f32], width: usize, height: usize,
                        levels: &[f32]) -> ContourLines
```

- マーチングスクエアズアルゴリズムで等高線を計算
- コマンドパレット → "Add Contour Overlay..."
- ソース：現在のタブ / 別のタブから選択
- レベル指定：自動（sigma倍数）/ パーセンタイル / 手動
- WCS 整合がある場合は座標変換して重ねる

#### 8-3. カタログオーバーレイ（ネットワーク対応）

ファイル：`crates/fitsview-core/src/catalog.rs`、`crates/fitsview-gui/src/catalog_overlay.rs`

```rust
pub struct CatalogSource {
    pub ra: f64,
    pub dec: f64,
    pub magnitude: Option<f32>,
    pub object_type: Option<String>,
    pub name: Option<String>,
    pub extra: HashMap<String, String>,
}

pub async fn query_simbad(ra: f64, dec: f64, radius_arcmin: f64, max_results: usize)
    -> anyhow::Result<Vec<CatalogSource>>

pub async fn query_vizier(catalog_id: &str, ra: f64, dec: f64, radius_arcmin: f64)
    -> anyhow::Result<Vec<CatalogSource>>

pub async fn query_gaia_dr3(ra: f64, dec: f64, radius_arcmin: f64, max_results: usize)
    -> anyhow::Result<Vec<CatalogSource>>
```

- HTTP クライアント：`reqwest`（非同期）
- クエリはバックグラウンドスレッドで実行、完了後に `mpsc` でGUIに通知
- 結果を `tab.catalogs: Vec<CatalogOverlay>` に追加
- ローカルVOTableファイルのパースも対応（`Ctrl+R` から選択）
- シンボルクリックでオブジェクト詳細ポップアップ
- フィルタリング：等級・タイプ・キーワード
- **オフラインキャッシュ**（UC-09）：クエリ結果を `~/.cache/fits-view/catalogs/<hash>.json` に保存、有効期限 24時間
  - キャッシュヒット時はネットワーク不要で即座に返す
  - オフライン時はキャッシュがあれば使用、なければエラーではなく空リスト + 通知バナー

**完了基準：**
- 3つのタブからRGB合成画像を生成できる
- コントゥアーが正しい位置に描画される
- WCS付きファイルでSIMBADクエリが動作し、結果が画像上に表示される
- ネットワーク切断状態でもキャッシュ済みクエリが正常に動作する

---

### Step 9 — 解析ツール群

**目的**：観測天文学者が日常的に使う解析機能を揃える。

#### 9-1. 1Dプロファイル抽出

ファイル：`crates/fitsview-core/src/profile.rs`、`crates/fitsview-gui/src/profile_tool.rs`

```rust
pub struct Profile {
    pub distances: Vec<f32>,  // ピクセル距離（またはWCS角距離）
    pub values: Vec<f32>,
    pub x0: f64, pub y0: f64,
    pub x1: f64, pub y1: f64,
}

pub fn extract_profile(data: &[f32], width: usize, height: usize,
                       x0: f64, y0: f64, x1: f64, y1: f64,
                       n_samples: usize) -> Profile

pub fn fit_gaussian_1d(profile: &Profile) -> Option<GaussianFit>

pub struct GaussianFit {
    pub amplitude: f32,
    pub center: f32,   // ピクセル単位
    pub sigma: f32,
    pub fwhm: f32,
    pub offset: f32,
}
```

- コマンドパレット → "Line Profile Tool"
- モード中：画像上をドラッグで直線を定義 → `plot_panel` にプロット表示
- WCSが有効な場合は角距離（arcsec / arcmin / deg）で軸表示
- Gaussian fit ボタン（PSF測定用）
- CSV エクスポート

#### 9-2. アパーチャーフォトメトリー

ファイル：`crates/fitsview-core/src/photometry.rs`、`crates/fitsview-gui/src/photometry_tool.rs`

```rust
pub struct ApertureResult {
    pub x: f64, pub y: f64,          // 中心（ピクセル）
    pub ra: Option<f64>, pub dec: Option<f64>,
    pub aperture_radius: f64,
    pub sky_inner: f64, pub sky_outer: f64,
    pub target_sum: f32,
    pub sky_mean: f32,
    pub net_flux: f32,
    pub snr: f32,
    pub magnitude: Option<f32>,      // FITS ヘッダのZPがある場合
}

pub fn aperture_photometry(
    data: &[f32], width: usize, height: usize,
    x: f64, y: f64,
    aperture_radius: f64,
    sky_inner: f64, sky_outer: f64,
) -> ApertureResult
```

- コマンドパレット → "Photometry Mode"
- クリックで円形アパーチャーを配置（内円・外環を描画）
- サイドパネルに測定結果テーブル（座標・フラックス・SNR・等級）
- 複数アパーチャーの一括配置
- CSV / VOTable エクスポート
- `MAGZPT` / `PHOTZP` キーワードがあれば等級換算

#### 9-3. 画像演算

ファイル：`crates/fitsview-core/src/arithmetic.rs`、`crates/fitsview-gui/src/arithmetic_panel.rs`

```rust
pub enum ArithOp { Add, Sub, Mul, Div, RelDiff }

pub fn image_arithmetic(
    a: &[f32], b: &[f32], width: usize, height: usize,
    op: ArithOp, scale: f32,
) -> anyhow::Result<Vec<f32>>
```

- コマンドパレット → "Image Arithmetic..."
- A と B にタブを選択、演算子を指定
- 画像サイズが異なる場合は警告 + キャンセル
- 結果を新タブで表示
- CLI 対応：`fits-view arithmetic a.fits - b.fits --output diff.fits`

#### 9-4. マスクオーバーレイ

ファイル：`crates/fitsview-gui/src/mask_overlay.rs`（region.rs の拡張）

```rust
pub struct MaskOverlay {
    pub data: Vec<u8>,    // 非ゼロ = マスク
    pub width: usize,
    pub height: usize,
    pub color: [u8; 4],  // RGBA
    pub bit_colors: HashMap<u8, [u8; 4]>,  // ビット値ごとの色
}
```

- コマンドパレット → "Load Mask..." で `Ctrl+R` ダイアログから選択
- 半透明赤でオーバーレイ（デフォルト）
- ビット値ごとに色分け可能（多ビットマスク対応）
- セッションに保存・復元

**完了基準：**
- 直線ドラッグでプロファイルが表示され、Gaussian fit が動作する
- クリックしてフラックスが測定でき、CSVにエクスポートできる
- 2枚のFITSを引き算して差分画像が新タブに表示される

---

#### 9-5. X線イベントリストのエネルギーバンドフィルター（UC-12）

**目的**：Phase 1 で実装した BINTABLE イベントリスト変換を拡張し、エネルギー帯域で絞り込んでカウントマップを再生成する。Chandra / XMM-Newton データの標準的な解析手順。

ファイル：`crates/fitsview-core/src/event_list.rs`（拡張）

```rust
pub struct EnergyFilter {
    pub emin: f64,        // eV または keV（ENERGY列単位に依存）
    pub emax: f64,
    pub column: String,   // 自動検出："ENERGY", "PI", "PHA" の優先順
}

/// イベントをエネルギー範囲でフィルタリング
pub fn filter_events_by_energy(
    events: &BintableEvents,
    filter: &EnergyFilter,
) -> BintableEvents

/// フィルタ後イベントをビニングしてカウントマップを生成
pub fn rebin_events(
    events: &BintableEvents,
    width: usize,
    height: usize,
    bin_size: usize,
) -> Vec<f32>
```

**エネルギー列の自動検出：**
- `ENERGY`（keV）→ `PI`（channel integer）→ `PHA`（pulse height）の順で検索
- 単位は `TUNIT` キーワードから読む（eV / keV / channel）
- 検出した列名と単位をパネルヘッダに表示

**UI：**
- イベントリストが検出されたタブの下部に「Energy Filter」パネルを自動表示
- emin / emax をデュアルスライダー + 数値入力で指定
- "Apply" ボタンでフィルター済みイベントからカウントマップを再計算・再表示
- 現在の選択範囲とイベント数（フィルタ前/後）をパネルに表示

**完了基準：**
- `ENERGY`（または `PI`, `PHA`）列を持つイベントリストで Energy Filter パネルが表示される
- emin / emax を変更して "Apply" するとカウントマップが更新される
- フィルタ適用後のカウントマップで通常の測光・プロファイル抽出が動作する

---

## フェーズ4：IDE・連携機能

---

### Step 10 — SAMP + スクリプトコンソール + 論文書き出し

#### 10-1. SAMP連携

ファイル：`crates/fitsview-gui/src/samp.rs`

SAMPはXML-RPCベースのプロトコル。`reqwest` と自前のXML-RPCハンドラーで実装する（または軽量外部クレート）。

```rust
pub struct SampClient {
    hub_url: String,
    private_key: String,
    pub connected: bool,
}

impl SampClient {
    pub fn connect() -> anyhow::Result<Self>
    pub fn disconnect(&mut self)
    pub fn send_image(&self, path: &Path) -> anyhow::Result<()>
    pub fn send_table(&self, votable_path: &Path) -> anyhow::Result<()>
    pub fn poll_messages(&mut self) -> Vec<SampMessage>
}

pub enum SampMessage {
    LoadImage(PathBuf),
    PointAt { ra: f64, dec: f64 },
    LoadTable(PathBuf),
}
```

- コマンドパレット → "SAMP → Connect" / "SAMP → Disconnect"
- 受信：`image.load.fits`, `coord.pointAt.sky`, `table.load.votable`
- 送信：カーソル座標のブロードキャスト、測光テーブルの送信

#### 10-2. スクリプトコンソール

ファイル：`crates/fitsview-gui/src/script_console.rs`

**実装方針：**
- フェーズ4では Lua（`mlua`）で実装（フットプリントが小さく、組み込みに向く）
- フェーズ5 で PyO3 Pythonとの統合を検討

```lua
-- fv オブジェクトで GUI にアクセス
local tab = fv.active_tab()
local data = tab:data()          -- float[]
local w, h = tab:size()

-- 新しい配列を作って表示
local result = {}
for i = 1, #data do
    result[i] = math.log10(data[i] + 1)
end
fv.show(result, w, h, {colormap="viridis", title="log10(image)"})

-- ヘッダ取得
local header = tab:header()
print(header["EXPTIME"])
```

**GUI：**
- `Ctrl+Shift+C` でコンソールを開く
- 入力欄 + 実行ボタン（`Enter` で実行）
- 出力欄（ログ + エラー表示）
- 履歴（↑↓キーで辿る）
- サンプルスクリプトのスニペット集

#### 10-3. 論文品質の書き出し

ファイル：`crates/fitsview-cli/src/main.rs`（render サブコマンド拡張）

```bash
# PDF出力（300dpi、カラーバー付き）
fits-view render image.fits \
  --output figure.pdf \
  --format pdf \
  --dpi 300 \
  --colorbar \
  --title "NGC 1234 Hα" \
  --colormap plasma \
  --scale zscale

# SVG出力（ベクター）
fits-view render image.fits --output figure.svg --format svg --wcs-grid
```

- `printpdf` クレートでPDF生成
- カラーバー、タイトル、軸ラベル（WCS）、スケールバー（角距離）
- DPI指定（72 / 150 / 300）

**完了基準：**
- TOPCATが起動中にSAMP接続でき、カーソル座標が連動する
- Luaスクリプトでピクセルデータを操作して新タブを作れる
- `fits-view render image.fits --output fig.pdf --format pdf` でPDFが生成される

---

## フェーズ5：配布・拡張

---

### Step 11 — Wasm対応

ファイル：`crates/fitsview-wasm/`（新規クレート）

- `trunk` + `wasm-bindgen` でブラウザ向けビルド
- `eframe` の `web_app` feature を使用
- `memmap2` は Wasm 非対応 → XHR Range リクエストでタイル取得
- GPU：WebGPU（Chrome 113+）または WebGL フォールバック
- 配布：単一 HTML + WASM（GitHub Pages / S3）

```bash
cd crates/fitsview-wasm && trunk build --release
trunk serve  # ローカル確認
```

**完了基準：**
- Chrome で FITS ファイルをブラウザ上で表示できる

---

### Step 12 — Pythonバインディング + パッケージング

#### 12-1. PyO3バインディング

ファイル：`crates/fitsview-py/`（新規クレート）

```python
import fitsview

img = fitsview.open("image.fits")
header = img.header()              # dict
data = img.data(hdu=1)            # np.ndarray[float32]
stats = img.stats()               # dict
viewer = fitsview.show(img, colormap="viridis", scale="zscale")
coords = viewer.pick()            # GUIで点を選択 → (x, y, value)
result = viewer.aperture(ra=134.52, dec=25.31, radius_arcsec=5.0)
```

- `maturin` でビルド、`PyPI` に公開
- `numpy` / `astropy` との相互運用

#### 12-2. パッケージング

- GitHub Actions でLinux / macOS / Windowsのリリースバイナリを自動生成
- `cargo-dist` によるリリース自動化
- Homebrew formula（macOS）
- `cargo install fits-view` の動作確認
- `pip install fitsview` の動作確認

#### 12-3. パフォーマンス最適化

- `criterion` ベンチマークを各クレートに追加
- タイルロード・レンダリング・WCS変換・ヒストグラム計算のベンチマーク
- KPI全指標の達成確認：
  - 起動 < 1秒
  - 10GB FITS 初回タイル < 2秒
  - GPU パン・ズーム 60fps
  - メモリ使用量 < 2GB（50GB FITSを閲覧中）
  - ヒストグラム更新 < 100ms

**完了基準：**
- `pip install fitsview` で Python バインディングがインストールできる
- KPI の全指標を達成

---

## 依存関係サマリー

```
[完了] Step 0-4（フェーズ1）
    ↓
Step 5（ヒストグラム / WCSグリッド / 分割連動 / CLIギャップ）
    ↓
Step 6（データキューブ）← プロットパネル（Step 9でも使用）
    ↓
Step 7（セッション管理 / アノテーション）
    ↓
Step 8（RGBコンポジット / コントゥアー / カタログ）← reqwest追加
    ↓
Step 9（プロファイル / 測光 / 画像演算 / マスク）
    ↓
Step 10（SAMP / スクリプトコンソール / PDF書き出し）
    ↓
Step 11（Wasm）
Step 12（PyO3 / パッケージング）  ← Step 11と並列可能
```

---

## リスクと対策

| リスク | 対策 |
|---|---|
| `fitsrs` が一部 FITS 方言に非対応 | 問題発生時は `fitsrs` に PR / フォーク、または独自パーサー部分実装 |
| イベントリストの列名が衛星ごとに異なる | 優先順位付きの列名候補リストで対応。未検出時はユーザーが手動選択できるUIを追加 |
| 大量イベント（1億件以上）のビニングが遅い | `rayon::par_iter` で並列化。`memmap2` と統合してゼロコピー読み込みに移行 |
| キューブデータ（NAXIS4）の多次元性 | まず NAXIS3 に集中し、NAXIS4 以降は「追加軸スライダー」として後から拡張 |
| WCS高精度変換（SIP / TPV歪み補正） | まず TAN 線形のみ実装し、精度要求が上がれば `wcslib` C ライブラリ FFI を追加 |
| カタログクエリのネットワーク依存 | タイムアウト設定、オフラインキャッシュ、エラー時の graceful degradation |
| Lua/Pythonスクリプトの安全性 | サンドボックス化、ファイルシステムアクセスは明示的な許可制に |
| SAMPプロトコルの実装コスト | まずは送受信の最小セット（`image.load.fits` / `coord.pointAt.sky`）に絞る |
| WSL2 で wgpu が動作しない | CPU-only ビルドで全フェーズ開発可。GPU は WSL2 `/dev/dxg` 経由で検証 |
| `memmap2` の Windows/WSL2 挙動差異 | `/mnt/c/` を回避し WSL2 ネイティブ FS で開発・テスト |

---

## 推奨開発順序（各 Step 内）

1. `fitsview-core` のデータ構造・ロジックを先に実装してユニットテスト
2. `fitsview-gui` の UI コンポーネントを追加して動作確認
3. `fitsview-cli` のコマンドを最後に統合

---

---

## ユースケース対応表

`usecases.md` の各 UC がどの Step で対応されるかの対照表。

| UC | タイトル | Step / 状態 | 追加された項目 |
|---|---|---|---|
| UC-01 | クイックルック | ✅ Phase 1 完了 | — |
| UC-02 | 画像品質評価 | Step 5, 9 | 飽和ピクセル数（Step 5-1 追加） |
| UC-03 | 複数ファイル比較 | Step 5 | ブリンク FPS 制御（Step 5-5 追加） |
| UC-04 | カラーマップ・スケール調整 | Step 5, 7 | — |
| UC-05 | WCS座標・グリッド | Step 5 | — |
| UC-06 | アパーチャー測光 | Step 9 | — |
| UC-07 | RGBコンポジット | Step 8 | — |
| UC-08 | コントゥアーオーバーレイ | Step 8 | — |
| UC-09 | カタログオーバーレイ | Step 8 | オフラインキャッシュ（Step 8-3 追加） |
| UC-10 | 1Dプロファイル抽出 | Step 9 | — |
| UC-11 | データキューブ閲覧 | Step 6 | — |
| UC-12 | X線イベントリスト | ✅ Phase 1 + Step 9 | エネルギーバンドフィルター（Step 9-5 追加） |
| UC-13 | DS9リージョンファイル | ✅ Phase 1 完了 | — |
| UC-14 | 画像演算 | Step 9 | — |
| UC-15 | バッドピクセルマスク | Step 9 | — |
| UC-16 | セッション管理 | Step 7 | — |
| UC-17 | アノテーション | Step 7 | `.reg` エクスポート（Step 7-2 追加） |
| UC-18 | 論文用図版書き出し | Step 10 | — |
| UC-19 | パイプライン CI/CD | ✅ Phase 1 + Step 5 | — |
| UC-20 | 大容量FITS閲覧 | ✅ Phase 1 完了 | — |
| UC-21 | SAMP連携 | Step 10 | — |
| UC-22 | スクリプトコンソール | Step 10 | — |
| UC-23 | Pythonライブラリ | Step 12 | — |
| UC-24 | ブラウザ版Wasm | Step 11 | — |

---

*最終更新：2026-05-31 | ステータス：フェーズ1完了・Step 5 計画中*
