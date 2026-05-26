# fits-view — コンセプト仕様書

> **"科学データを、科学者のスピードで。"**
> 超軽量・超高速・次世代FITSビューア（Rust製）

> **注記：** プロダクト名 `fits-view` は仮称。正式名称は後日決定予定。

---

## 1. プロジェクト概要

### 背景と課題

天文学における標準データ形式FITSは、1981年の策定以来40年以上にわたり使われ続けている。しかし、既存のビューアツールは以下の問題を抱えている。

| 問題 | 代表的な既存ツール |
|---|---|
| UIが1990年代のまま（Tcl/Tk） | DS9, SAOImage |
| RAM依存で大型ファイルが扱えない | DS9, AstroImageJ |
| GUIが孤立しCLI/スクリプトと連携不可 | ほぼ全ツール |
| 起動が遅い（JVM依存など） | APT, AstroImageJ |
| Wasmに非対応（インストール必須） | 全ツール |
| ディレクトリ単位での作業が困難 | 全ツール |

SKA（Square Kilometre Array）、Rubin Observatory（LSST）、Subaru HSCなど次世代観測装置は、1ファイルで数十GB〜数TBのFITSデータを生成する。既存ツールはこのスケールに根本的に対応できない。

### ビジョン

**fits-viewは、巨大データを軽快に扱い、CLIやスクリプトと自然に統合できる、天文学者のためのモダンな作業ツールである。**

VSCodeのような直感的なUIでディレクトリを丸ごと開き、複数のFITSファイルをタブで管理しながら、パイプラインの一員として動作できる。

---

## 2. ターゲットユーザー

### プライマリ

- **観測天文学者**：望遠鏡データのクイックルック、品質確認
- **データパイプラインエンジニア**：CI/CDでの自動サムネイル生成、検査
- **大学院生・研究者**：日常的なデータ閲覧・解析プレビュー

### セカンダリ

- 天文ソフトウェア開発者（Pythonライブラリとの統合）
- プラネタリウム・教育機関（Wasm版）

---

## 3. 差別化戦略

### コアバリュー（4本柱）

```
┌─────────────────────────────────────────────────────┐
│  1. 超高速・超軽量     GPU駆動（CPU自動フォールバック）│
│  2. 巨大データ対応     TB級FITSをゼロコピーで         │
│  3. CLI/スクリプト統合  パイプラインの一員として       │
│  4. VSCodeライクUI     ディレクトリ・複数ファイル管理  │
└─────────────────────────────────────────────────────┘
```

### 競合比較

| 機能 | fits-view | DS9 | FITS Liberator | takefits |
|---|:---:|:---:|:---:|:---:|
| 起動時間 < 1秒 | ✅ | ❌ | ❌ | ❌ |
| TB級ファイル対応 | ✅ | ❌ | △ | ❌ |
| GPU描画（CPU自動FB） | ✅ | ❌ | ❌ | ❌ |
| CLIモード（`fits-view .`）| ✅ | △ | ❌ | ❌ |
| ヘッドレス実行 | ✅ | ❌ | ❌ | ❌ |
| Wasm対応 | ✅（予定）| ❌ | ❌ | ❌ |
| Python連携 | ✅ | △ | ❌ | ✅ |
| セッション保存 | ✅ | △ | ❌ | ✅ |
| ディレクトリ単位で開く | ✅ | ❌ | ❌ | ❌ |
| 複数ファイルのタブ管理 | ✅ | △ | ❌ | ❌ |

---

## 4. 機能仕様

### 4.1 VSCodeライクUI

fits-viewのGUIはVS Codeの操作感をベースに設計する。天文学者がコードエディタを扱う感覚でFITSデータを管理できることを目指す。

#### レイアウト構成

```
┌──────────────────────────────────────────────────────────┐
│  [メニューバー]  File  View  Tools  Window  Help          │
├────────────┬─────────────────────────────┬───────────────┤
│            │ [タブバー]                   │               │
│  ファイル  │ image_r.fits × │ image_g.fits ×│ ...         │
│  エクスプ  ├─────────────────────────────┤  ヘッダ       │
│  ローラー  │                             │  ビューア     │
│            │                             │               │
│  ─────     │      メイン表示エリア        │  NAXIS = 2    │
│  📁 obs/   │      （FITSイメージ）        │  NAXIS1= 4096 │
│  📁 cal/   │                             │  ...          │
│  📄 a.fits │                             │               │
│  📄 b.fits ├─────────────────────────────┤               │
│            │  [ステータスバー]            │               │
│            │  RA: 134.52  Dec: 25.31     │               │
└────────────┴─────────────────────────────┴───────────────┘
```

#### ファイルエクスプローラー（サイドバー）

- ディレクトリツリーをサイドパネルで表示
- `.fits` / `.fit` / `.fits.gz` ファイルを自動認識・ハイライト
- ファイルをクリックするとタブで開く
- 右クリックコンテキストメニュー：開く / サムネイル生成 / 情報表示 / 比較表示
- ディレクトリウォッチ：新ファイル追加を自動検知して一覧更新

#### マルチタブ管理

- 複数FITSファイルをタブで同時に開く
- タブのドラッグ&ドロップによる並び替え
- タブを右クリック → 分割表示（縦/横）
- `Ctrl+Tab` でタブ切り替え
- `Ctrl+W` でタブを閉じる

#### 分割表示・比較モード

```
[Side-by-Side]          [Grid 2x2]
┌──────┬──────┐         ┌──────┬──────┐
│  A   │  B   │         │  A   │  B   │
│      │      │         ├──────┼──────┤
└──────┴──────┘         │  C   │  D   │
                        └──────┴──────┘
```

- 2〜4ファイルを並べて比較
- 同期パン・ズーム（リンクモード）
- カーソル位置を全パネルに同期表示

#### キーボードショートカット

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

---

### 4.2 巨大データ対応（Large-File Support）

#### メモリマップI/O

```
ファイル全体をRAMに載せない。必要なタイルだけを読む。

FITS File (50GB)
     │
     ▼
 memmap2 (OS管理)
     │
     ├─ Tile (0,0) ──→ GPU/CPU Texture
     ├─ Tile (1,0) ──→ GPU/CPU Texture
     └─ Tile (n,m) ──→ GPU/CPU Texture（オンデマンド）
```

- `memmap2` クレートによるOSレベルのメモリマッピング
- タイルサイズ：デフォルト512×512px（設定変更可）
- LRUキャッシュによるタイル管理（メモリ上限を設定可能）
- 圧縮FITSは非同期展開（Rayon並列）

#### 対応ファイル規模の目標

| ファイルサイズ | 動作目標 |
|---|---|
| < 1 GB | 即時表示（< 500ms） |
| 1 GB – 10 GB | 初回タイル表示 < 2秒 |
| 10 GB – 1 TB | パン・ズームはスムーズ、初期読み込みは進捗表示 |
| > 1 TB | タイルストリーミングモード（要設定） |

#### HDU（Header Data Unit）ナビゲーション

- 全HDU一覧をサイドパネルに表示
- イメージ型・テーブル型・圧縮型を自動判別
- HDU間のクイックスイッチ（キーボードショートカット）

---

### 4.3 CLIモード（Command-Line Interface）

fits-viewは**GUIとCLIの両モードを持つ単一バイナリ**として配布する。

`code .` と同様の感覚で使えることを重視する。

#### 基本コマンド構造

```bash
fits-view [OPTIONS] [PATH] [COMMAND]
```

#### GUIモード（デフォルト）

```bash
# カレントディレクトリをエクスプローラーで開く（code . と同様）
fits-view .

# ディレクトリを指定して開く
fits-view /data/obs/2026-05-25/

# ファイルを直接開く
fits-view image.fits

# 複数ファイルをタブで開く
fits-view a.fits b.fits c.fits

# 特定のHDUを開く
fits-view image.fits --hdu 2

# 複数ファイルを並べて表示
fits-view a.fits b.fits --layout side-by-side

# セッションファイルを復元
fits-view --session last_session.fvs
```

#### ヘッドレスモード（GUIなし）

```bash
# サムネイル生成（CI/CDパイプライン用途）
fits-view render image.fits --output thumb.png --size 256x256

# ディレクトリ内の全FITSファイルを一括サムネイル生成
fits-view render /data/obs/ --output-dir ./thumbs/ --format png

# カラースケールを指定して書き出し
fits-view render image.fits \
  --colormap viridis \
  --scale log \
  --zscale \
  --output science_fig.pdf

# ヘッダ情報をJSON出力
fits-view info image.fits --format json

# 統計情報を出力
fits-view stats image.fits --hdu 1 --region "100:200,100:200"
```

#### パイプライン連携

```bash
# stdout への出力（他ツールへのパイプ）
fits-view stats image.fits --format csv | python analyze.py

# 終了コードで品質チェック
fits-view check image.fits --assert "NAXIS == 2" || exit 1

# バッチ処理（並列）
ls *.fits | xargs -P 8 fits-view render --output-dir ./thumbs/
```

#### Python連携インターフェース

```python
import subprocess, json

# ヘッダ取得
result = subprocess.run(
    ["fits-view", "info", "image.fits", "--format", "json"],
    capture_output=True, text=True
)
header = json.loads(result.stdout)

# または、将来的なPythonバインディング（PyO3）
import fitsview
viewer = fitsview.open("image.fits")
viewer.show(hdu=1, colormap="viridis")
coords = viewer.pick()  # GUIで点を選択 → 座標を返す
```

---

### 4.4 描画エンジン（GPU / CPU 自動切替）

GPU（wgpu対応デバイス）がある場合はGPUレンダリングを優先し、ない場合はCPUソフトウェアレンダリングに自動フォールバックする。

```
CPU (fitsrs / memmap2)
    │  生データ（f32/f64/i16など）
    ▼
レンダラー自動選択
    ├─ [GPU利用可能]
    │       Transfer Buffer (wgpu)
    │           │
    │           ▼
    │       Compute Shader
    │           ├─ トーンマッピング
    │           ├─ カラーマップ適用（1D LUTテクスチャ）
    │           └─ WCSグリッド計算
    │           │
    │           ▼
    │       Fragment Shader → 画面表示
    │
    └─ [GPU利用不可 / フォールバック]
            CPU Software Renderer (rayon並列)
                ├─ トーンマッピング
                ├─ カラーマップ適用
                └─ WCSグリッド計算
                │
                ▼
            RGBA バッファ → egui Texture → 画面表示
```

#### レンダラー選択ロジック

| 条件 | 使用レンダラー |
|---|---|
| wgpuデバイスが存在する | GPUレンダラー |
| wgpuデバイスが存在しない（SSH環境・仮想環境など） | CPUレンダラー |
| GPU OOM / レンダリングエラー発生時 | 自動でCPUフォールバック |
| ユーザーが `--renderer cpu` を明示指定 | CPUレンダラー強制 |

- **対応データ型**：BITPIX 8/16/32/64/-32/-64 全対応
- **カラーマップ**：グレー, viridis, plasma, inferno, hot, rainbow, DS9互換
- **スケーリング**：linear, log, sqrt, asinh, zscale（自動）, minmax
- **フレームレート目標**：GPU時 60fps以上 / CPU時 30fps以上（512px以下のタイルで）

---

### 4.5 セッション管理と再現性

```json
// session.fvs（fits-view Session）形式
{
  "version": "1.0",
  "workspace": "/data/obs/2026-05-25/",
  "files": [
    {
      "path": "/data/obs/image_r.fits",
      "hdu": 1,
      "display": {
        "colormap": "viridis",
        "scale": "log",
        "vmin": 100.5,
        "vmax": 4200.0,
        "zoom": 2.5,
        "center_wcs": [134.5, 25.3]
      },
      "annotations": [
        {"type": "circle", "ra": 134.52, "dec": 25.31, "radius_arcsec": 5.0, "label": "Target"}
      ]
    }
  ],
  "layout": "side-by-side"
}
```

- セッションはJSON（`.fvs`）で保存・共有可能
- コマンドラインから直接復元：`fits-view --session obs_20260525.fvs`
- 表示設定のエクスポート：論文掲載品質のSVG/PDF書き出し
- ワークスペース単位でセッション管理（VS Codeのワークスペース概念に対応）

---

### 4.6 WCS（World Coordinate System）対応

- 赤経・赤緯のグリッドオーバーレイ
- カーソル位置のWCS座標リアルタイム表示
- カタログオーバーレイ（将来対応）：SIMBAD、VizieR、Gaia DR3

---

## 5. 技術スタック

### コア

| 役割 | クレート / 技術 |
|---|---|
| FITSパーサ | `fitsrs`（CDS製、純Rust） |
| メモリマップI/O | `memmap2` |
| GPU描画 | `wgpu`（WebGPU準拠、GPU利用可能時） |
| CPUフォールバック描画 | `rayon` + カスタムソフトウェアレンダラー |
| GUI | `egui` + `eframe` |
| 並列処理 | `rayon` |
| 非同期 | `tokio` |
| CLIパーサ | `clap` v4 |
| シリアライズ | `serde` + `serde_json` |
| ディレクトリウォッチ | `notify` |

### 将来対応

| 役割 | 候補 |
|---|---|
| Wasm対応 | `trunk` + `wasm-bindgen` |
| Pythonバインディング | `PyO3` |
| プラグインAPI | Wasm（`wasmtime`）またはLua（`mlua`） |

---

## 6. アーキテクチャ概要

```
┌──────────────────────────────────────────────────────┐
│                  fits-view binary                    │
│                                                      │
│  ┌─────────────┐        ┌──────────────────────┐    │
│  │  CLI Layer  │        │     GUI Layer        │    │
│  │  (clap)     │        │  (egui + eframe)     │    │
│  │             │        │  ┌────────────────┐  │    │
│  │  fits-view .│        │  │ File Explorer  │  │    │
│  │  fits-view  │        │  │ (サイドバー)   │  │    │
│  │   render ...|        │  ├────────────────┤  │    │
│  │             │        │  │ Tab Manager    │  │    │
│  └──────┬──────┘        │  │ (マルチタブ)   │  │    │
│         │               │  ├────────────────┤  │    │
│         │               │  │ Split View     │  │    │
│         │               │  │ (分割表示)     │  │    │
│         │               └──────────┬───────────┘    │
│         └──────────┬───────────────┘                │
│                    │                                │
│         ┌──────────▼───────────┐                    │
│         │    Core Engine       │                    │
│         │  ┌────────────────┐  │                    │
│         │  │  FITS I/O      │  │                    │
│         │  │  (fitsrs +     │  │                    │
│         │  │   memmap2)     │  │                    │
│         │  └───────┬────────┘  │                    │
│         │          │           │                    │
│         │  ┌───────▼────────┐  │                    │
│         │  │  Tile Manager  │  │                    │
│         │  │  (LRU Cache)   │  │                    │
│         │  └───────┬────────┘  │                    │
│         │          │           │                    │
│         │  ┌───────▼────────┐  │                    │
│         │  │  Renderer      │  │                    │
│         │  │  ┌──────────┐  │  │                    │
│         │  │  │GPU(wgpu) │  │  │                    │
│         │  │  │ ↕ auto   │  │  │                    │
│         │  │  │CPU(rayon)│  │  │                    │
│         │  │  └──────────┘  │  │                    │
│         │  └────────────────┘  │                    │
│         └──────────────────────┘                    │
└──────────────────────────────────────────────────────┘
```

---

## 7. 開発環境

### 動作環境

- **開発環境**：WSL2 Ubuntu（Linux）
- **ターゲットプラットフォーム**：Linux / macOS / Windows
- **Rustバージョン**：stable（MSRV: 1.75以上）

### WSL2開発時の注意点

- **GPU利用**：WSL2でwgpuを使う場合、DirectX 12経由（`dx12`バックエンド）またはVulkan（Mesa D3D12）を使用。WSL2のGPUパスは`/dev/dxg`経由でアクセス可能。
- **GPUが利用できない場合**：自動的にCPUソフトウェアレンダリングへフォールバック。SSH接続・CI環境でも動作保証。
- **ファイルシステム**：Windowsのファイル（`/mnt/c/...`）へのmemmap2アクセスはパフォーマンス低下の可能性あり。開発・テスト時はWSL2ネイティブファイルシステム（`/home/...`）を推奨。
- **Windowsフォントとの統合**：egui向けにWSL2フォントパスの設定が必要な場合あり。

### ビルド・実行

```bash
# 通常ビルド
cargo build --release

# GPU有効でビルド（デフォルト）
cargo build --release --features gpu

# CPUのみモード（GPU機能を無効化）
cargo build --release --no-default-features --features cpu-only

# 実行
./target/release/fits-view .
./target/release/fits-view /mnt/c/Users/.../data/
```

---

## 8. 開発ロードマップ

### Phase 1 — MVP（最初の動くもの）

- [ ] `fitsrs` でFITS読み込み
- [ ] `egui` でイメージ表示（小〜中規模ファイル）
- [ ] zscale自動スケーリング
- [ ] パン・ズーム操作
- [ ] ヘッダビューア
- [ ] CPUレンダリングによる基本表示（GPU不要で動作確認可能）

### Phase 2 — VSCodeライクUI

- [ ] ファイルエクスプローラー（サイドバー）
- [ ] マルチタブ管理
- [ ] `fits-view .` でディレクトリを開く
- [ ] 分割表示・比較モード
- [ ] キーボードショートカット一式

### Phase 3 — 巨大データ対応

- [ ] `memmap2` によるタイルストリーミング
- [ ] LRUタイルキャッシュ
- [ ] 非同期タイルロード
- [ ] 進捗インジケーター
- [ ] GPU描画レイヤー追加（wgpu）、CPU自動フォールバック実装

### Phase 4 — CLIモード

- [ ] `clap` によるCLI構築
- [ ] ヘッドレスレンダリング（`render` サブコマンド）
- [ ] `info` / `stats` サブコマンド（JSON/CSV出力）
- [ ] バッチ処理対応

### Phase 5 — 統合・拡張

- [ ] セッション保存・復元（`.fvs`形式）
- [ ] WCSグリッドオーバーレイ
- [ ] PDF/SVG書き出し
- [ ] Wasm対応（ブラウザ版）
- [ ] PyO3 Pythonバインディング

---

## 9. 成功指標（KPI）

| 指標 | 目標値 |
|---|---|
| 起動時間（1GB FITSを開くまで） | < 1秒 |
| 10GB FITSの最初のタイル表示 | < 2秒 |
| パン・ズーム操作のフレームレート（GPU） | 60fps |
| パン・ズーム操作のフレームレート（CPU） | 30fps以上（512px以下タイル） |
| バイナリサイズ（strip後） | < 20MB |
| `fits-view render` スループット | > 50ファイル/分（並列） |
| メモリ使用量（50GB FITSを閲覧中） | < 2GB |

---

## 10. ライセンス・配布

- **ライセンス**：MIT または Apache-2.0（デュアルライセンス）
- **配布**：
  - バイナリリリース（GitHub Releases）：Linux / macOS / Windows
  - `cargo install fits-view`
  - Homebrew formula（macOS）
  - 将来：`pip install fitsview`（PyO3バインディング同梱）

---

## 11. 参考・関連プロジェクト

| プロジェクト | 関係 |
|---|---|
| [fitsrs](https://github.com/cds-astro/fitsrs) | FITSパーサとして採用予定 |
| [rustronomy](https://github.com/smups/rustronomy) | エコシステム参照 |
| [egui](https://github.com/emilk/egui) | GUIフレームワーク |
| [rerun](https://github.com/rerun-io/rerun) | アーキテクチャ参考（egui + wgpu大規模事例） |
| [VS Code](https://code.visualstudio.com/) | UIデザイン参考（ファイルエクスプローラー・タブ・分割表示） |
| [DS9](https://sites.google.com/cfa.harvard.edu/saoimageds9) | 主要競合・UXアンチパターン参考 |
| [FITS Liberator 5](https://noirlab.edu/public/products/fitsliberator/) | 競合（2025年11月v5.0リリース） |

---

*最終更新：2026-05-26 | ステータス：コンセプト段階*
