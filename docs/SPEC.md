# AVD Launcher — 実装仕様書

Rustで実装するAndroid開発者向けCLIツール。emulatorの選択・起動と、gradlewビルド・logcatストリーミングを提供する。

## 1. 概要

### 1.1 目的

Android開発ワークフローの日常的な操作(emulator選択・起動、ビルド、インストール、ログ確認)を、単一のCLIバイナリで完結させる。Android Studioに依存せず、ターミナル中心の開発フローに組み込めるようにする。

### 1.2 スコープ

- **含む**: emulator一覧取得と対話的選択、detached起動、gradle installDebug実行、logcatフィルタリングとストリーミング、ターミナル出力のカラーリング
- **含まない**: AVDの作成・削除、SDK自体のインストール、Gradle以外のビルドシステム、iOS向けの機能

### 1.3 サポートプラットフォーム

macOS、Linux、Windows。cross-platformを維持する。

---

## 2. コマンド仕様

### 2.1 サブコマンド構成

`clap` の derive featureを使ってサブコマンドを定義する。サブコマンド省略時は `launch` を実行する。

```
avd                       # デフォルト: launch と同じ
avd launch                # AVD選択して起動
avd run [OPTIONS]         # gradle install + logcatストリーミング
```

### 2.2 `avd launch`

引数なし。以下を実行する:

1. `emulator` バイナリを探す(2.5参照)
2. `emulator -list-avds` でAVD一覧取得
3. `dialoguer::Select` で一覧表示(数字キーと矢印キー両対応)
4. `dialoguer::Confirm` でCold Boot確認(デフォルト No)
5. 選択されたAVDを detached で起動

### 2.3 `avd run`

以下のフラグを持つ:

| フラグ | 型 | 説明 |
|-------|---|------|
| `--no-install` | bool | Gradleビルドをスキップし、既にインストール済みのAPKに対してlogcatだけ実行 |
| `--no-start` | bool | インストール後の自動アクティビティ起動をスキップ |
| `--clear` | bool | logcatバッファをクリアしてからストリーミング開始 |

実行フロー:

1. CWDから上位に向かって `gradlew` を探す(見つからなければエラー)
2. `adb` を検出、オンラインデバイス確認、なければboot完了まで待機
3. `--no-install` でなければ `./gradlew installDebug` 実行
4. ビルドされたAPKから `aapt dump badging` で `applicationId` を取得
5. `--no-start` でなければ `adb shell monkey` でメインアクティビティ起動
6. `--clear` なら `adb logcat -c`
7. `adb logcat -v brief --pid=<PID>` をストリーミング(Ctrl-Cで停止)

---

## 3. アーキテクチャ

### 3.1 ファイル構成

```
avd-launcher/
├── Cargo.toml
└── src/
    ├── main.rs      # clapによるサブコマンド定義とdispatch
    ├── android.rs   # 共通ユーティリティ: SDK検出、adb/emulator呼び出し、aapt解析
    ├── launch.rs    # `avd launch` の実装(dialoguer選択とdetached spawn)
    └── run.rs       # `avd run` の実装(gradle + logcatパイプライン)
```

### 3.2 依存クレート

```toml
[dependencies]
anyhow = "1"                                          # エラーハンドリング
clap = { version = "4", features = ["derive"] }       # サブコマンド定義
console = "0.15"                                      # 出力スタイリング、ANSIサポート
dialoguer = "0.11"                                    # Select, Confirmプロンプト
indicatif = "0.17"                                    # spinner
ctrlc = "3"                                           # Ctrl-Cハンドラ

[target.'cfg(unix)'.dependencies]
libc = "0.2"                                          # setsid() を呼ぶため
```

**採用理由**: ratatuiは検討したが、full-screen TUIとして持続的に画面を占有するツール(htop, lazygit等)向け。本ツールは短命なCLIなので、cargo/uv/ripgrepが採用しているCLIスタイル(dialoguer + indicatif + console)が適切。

---

## 4. 実装詳細

### 4.1 Android SDK バイナリの検出 (`android::locate_sdk_binary`)

以下の順序で探索する。最初にヒットしたパスを返す:

1. `$ANDROID_HOME/<subdir>/<name>[.exe]`
2. `$ANDROID_SDK_ROOT/<subdir>/<name>[.exe]`
3. プラットフォームデフォルト:
   - macOS: `~/Library/Android/sdk/<subdir>/<name>`
   - Windows: `~/AppData/Local/Android/Sdk/<subdir>/<name>.exe`
   - Linux: `~/Android/Sdk/<subdir>/<name>`
4. `PATH` 環境変数を順に走査

### 4.2 `aapt` の特殊扱い (`android::locate_aapt`)

`aapt` は `build-tools/<バージョン>/aapt` に配置されている。バージョンディレクトリを列挙し、辞書順で最新のものを選ぶ。

### 4.3 AVD detached起動 (`launch.rs`)

Command構築時の注意点:

- `stdin`, `stdout`, `stderr` を `Stdio::null()` で切断
- `current_dir` を emulator バイナリの親ディレクトリに設定(qemuサブバイナリをsibling位置で解決するため)
- Unixでは `pre_exec` で `libc::setsid()` を呼び、新セッションで起動してターミナル切断時のSIGHUPを回避
- Windowsでは `creation_flags` に `DETACHED_PROCESS (0x08) | CREATE_NEW_PROCESS_GROUP (0x200)` を設定

`pre_exec` 内は `unsafe` が必要(fork/exec間で非async-signal-safeな関数を呼ぶと未定義動作になるため)。`setsid` は許可された関数なので安全。

### 4.4 Cold Boot処理

`emulator` に `-no-snapshot-load` フラグを追加する。snapshotの読み込みだけスキップし、書き込みは通常通り行われる。完全にsnapshotを無効化したい場合は `-no-snapshot` だが、本ツールでは前者を採用。

### 4.5 gradlewの探索 (`run::find_gradle_root`)

CWDから開始して、親ディレクトリを辿りながら `gradlew` (Windowsでは `gradlew.bat`) が存在するディレクトリを探す。gitの `.git` 探索と同じロジック。ルートまで到達しても見つからなければエラー。

### 4.6 Gradle install実行 (`run::gradle_install`)

以下の設定でサブプロセスを起動する:

- `current_dir(project_root)` で `gradlew` があるディレクトリで実行
- 引数: `installDebug` と `--console=plain`
- 環境変数: `ANDROID_SERIAL=<serial>` で対象デバイスを固定

`--console=plain` の理由: Gradleのリッチな出力はANSIカーソル移動を使うため、`indicatif` のspinnerと画面の同じ行を奪い合って表示が崩れる。プレーン出力にしてこちらでパースする。

stdout/stderrを両方 `Stdio::piped()` で捕捉:

- **stdout**: 別スレッドで行単位に読み、`> Task ` プレフィックスを含む行を検出してspinnerメッセージを更新する。これで現在実行中のタスク(例: `:app:compileDebugKotlin`)がリアルタイム表示される
- **stderr**: 別スレッドで全行をVecに集める。ビルド失敗時のみeprintlnで赤色表示する

`ProgressStyle` テンプレート: `"{spinner:.green} {prefix:.bold} {wide_msg}"`。tickは `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`(braille dots)。

### 4.7 デバイス起動待機 (`android::wait_for_boot`)

2段階のポーリング:

1. `adb devices` で "device" 状態のシリアルが現れるまで待つ
2. そのシリアルに対して `adb shell getprop sys.boot_completed` を実行し、`1` が返るまで待つ

いずれも500msスリープでポーリング。タイムアウトは3分(180秒)。

### 4.8 パッケージ名の取得 (`android::package_name_from_apk`)

`aapt dump badging <APK>` の出力から:

```
package: name='com.example.app' versionCode='1' ...
```

の形式で `name='...'` を抽出する。`applicationIdSuffix`、product flavor、build variantすべてを考慮した実際のapplicationIdが得られるため、`build.gradle` のパースより信頼できる。

### 4.9 メインアクティビティの起動

`adb shell monkey -p <pkg> -c android.intent.category.LAUNCHER 1` を使う。理由: LAUNCHERカテゴリのactivityを自動検出してくれるため、マニフェストからmain activity名をパースする必要がない。

### 4.10 logcatストリーミング (`run::stream_logcat`)

1. `adb shell pidof <pkg>` を最大10秒間ポーリングしてPIDを取得(300msインターバル)
2. `adb logcat -v brief --pid=<PID>` を `Stdio::piped()` でspawn
3. `BufReader` で行単位に読み、`colorize_logcat` で先頭のpriority文字を色付けして出力
4. Ctrl-Cハンドラで `AtomicBool` を立て、ループで検出したら子プロセスを `kill()` して終了

**PIDフィルタリングの制限**: PIDはlogcat起動時に一度取得するのみ。アプリを再起動するとPIDが変わり、logcatは古いPIDのまま(=何も表示されない)になる。これは既知の制限として仕様に含める。

将来の改善案として、`pidof` を定期的に再実行してPID変化を検知したらlogcatを再spawnする方式が考えられる。

### 4.11 logcatカラーリング (`run::colorize_logcat`)

`-v brief` フォーマット: `<P>/<TAG>(<PID>): <MSG>`。先頭文字がpriority:

| Priority | 意味 | 色 |
|----------|------|-----|
| V | Verbose | dim |
| D | Debug | blue |
| I | Info | green |
| W | Warn | yellow |
| E | Error | red bold |
| F | Fatal | red bold on white |

先頭1文字のみをstyleでラップし、残りはそのまま連結する。

### 4.12 Ctrl-Cハンドリング

`ctrlc::set_handler` は1プロセスにつき1回しか呼べない。`stream_logcat` 内でのみ設定する。`Arc<AtomicBool>` でループに伝搬させ、子プロセスを明示的に `kill()` してから抜ける。

---

## 5. エラーハンドリング方針

- 全関数の戻り値は `anyhow::Result<T>`
- ユーザーの操作ミスに起因するエラー(gradlew見つからない、AVDが1つもない等)は `bail!` で明確なメッセージを出す
- SDK関連の検出失敗は「$ANDROID_HOMEを設定してください」等の具体的な修復手順を含める
- 内部エラーは `.context()` で操作内容を添える(例: `.context("running \`emulator -list-avds\`")`)

---

## 6. 出力スタイルガイド

`console::style` を使用:

- **情報行**: `›` (dim) プレフィックス + ラベル + cyanで値
  例: `› project: /Users/soma/Projects/myapp`
- **成功**: `✓` (green bold) プレフィックス
- **警告**: `!` (yellow) プレフィックス
- **失敗**: `✗` (red) プレフィックス、または `BUILD FAILED` (red bold)

---

## 7. パニック対策

TUIを使わないためterminal復元は不要。ただし将来的にダイアログの途中でパニックが起きるとdialoguerがカーソルを隠したまま終わる可能性がある。必要ならmainの先頭に:

```rust
let original = std::panic::take_hook();
std::panic::set_hook(Box::new(move |info| {
    let _ = console::Term::stdout().show_cursor();
    original(info);
}));
```

を入れる。優先度は低い。

---

## 8. テスト方針

CI環境にはAndroid SDKもemulatorも無いことが多いため、統合テストは難しい。以下を推奨:

- **ユニットテスト**: `colorize_logcat`, `find_gradle_root`(tempdirを使う), `package_name_from_apk` のパース部分
- **手動確認項目**:
  - AVD一覧が正しく取得できるか
  - Cold Bootオプションが実際にsnapshot無視になっているか
  - Gradleビルド中のspinnerメッセージが `:app:xxx` タスク名で更新されるか
  - logcatの色付けが正しいか
  - Ctrl-Cで子プロセス(adb)がゾンビにならないか

---

## 9. 既知の制限と将来の改善案

| 制限 | 影響 | 改善案 |
|-----|------|-------|
| PIDフィルタが起動時1回のみ | アプリ再起動でlog表示が止まる | pidofポーリング + logcat再spawn |
| 単一モジュール前提 | multi-moduleプロジェクトでAPKパスが違う | `--module <name>` フラグ追加 |
| `--console=plain` でGradle出力が簡素 | Gradleネイティブの進捗表示が見られない | `--raw` フラグでspinner無効化 |
| 複数デバイス選択不可 | 実機とemu両方接続時、gradle側は `ANDROID_SERIAL` で解決するが `adb devices` の一番目を使う | デバイス選択プロンプト追加 |
| Windowsでのlogcat色付け | 古いWindows Terminalでは色が出ない可能性 | `console` クレートが自動判定するので基本OK |

---

## 10. Cargo.toml 完全形

```toml
[package]
name = "avd-launcher"
version = "0.2.0"
edition = "2021"
description = "Interactive launcher for Android Virtual Devices + gradle install/logcat helper"

[[bin]]
name = "avd"
path = "src/main.rs"

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
console = "0.15"
dialoguer = "0.11"
indicatif = "0.17"
ctrlc = "3"

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[profile.release]
strip = true
lto = true
codegen-units = 1
```

---

## 11. 実装順序の推奨

Agentに実装させる場合、以下の順序でPRを分けると各段階でreviewable:

1. `Cargo.toml` + `main.rs` の骨組み(clapサブコマンド定義のみ、bodyはtodo!())
2. `android.rs` の SDK検出関数群(`locate_sdk_binary`, `locate_aapt`, `list_avds`, `list_online_devices`, `wait_for_boot`, `package_name_from_apk`)
3. `launch.rs` の完全実装
4. `run.rs` の gradle install部分(logcatはstub)
5. `run.rs` の logcat streaming部分
6. カラーリング、spinnerスタイル、UI文言の仕上げ

各ステップで `cargo check` が通ることを確認しながら進める。

---

## 12. Agentへの引き継ぎメモ

- Rustツールチェインが未インストールの場合、`rustup` で `stable` を入れて `cargo build --release` する
- 動作確認には Android SDK と少なくとも1つのAVDが必要
- `ANDROID_HOME` が設定されていない環境ではフォールバックパス経由になるが、CIでは明示設定推奨
- 実装中に判断に迷ったら、まず「cargo/uvがどうしているか」を参考基準にする(dialoguer/indicatifのeco-systemはこの2つが事実上のリファレンス実装)
