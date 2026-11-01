# EPMA System Data

App nativo para macOS, escrito em Rust ([egui/eframe](https://github.com/emilk/egui)), que mede e limpa o que o macOS mostra como **Dados do Sistema** (System Data) em Ajustes > Geral > Armazenamento.

## O que ele limpa

| Categoria | Local | Padrão |
|---|---|---|
| Caches de apps | `~/Library/Caches` | marcado |
| Logs e relatórios de falha | `~/Library/Logs` | marcado |
| Xcode DerivedData | `~/Library/Developer/Xcode/DerivedData` | marcado |
| Device Support | `~/Library/Developer/Xcode/* DeviceSupport` | marcado |
| Caches do Simulator | `~/Library/Developer/CoreSimulator/Caches` | marcado |
| Simuladores indisponíveis | `xcrun simctl delete unavailable` | marcado |
| Snapshots locais do Time Machine | `tmutil thinlocalsnapshots /` | marcado |
| Anexos baixados do Mail | `~/Library/Containers/com.apple.mail/.../Mail Downloads` | marcado |
| Caches de ferramentas dev | `~/.npm/_cacache`, `~/.gradle/caches`, `~/.cargo/registry/cache`, `~/.cache` | revisar |
| Xcode Archives | `~/Library/Developer/Xcode/Archives` | revisar |
| Backups de iPhone/iPad | `~/Library/Application Support/MobileSync/Backup` | revisar |

Categorias "revisar" começam desmarcadas. Dá para expandir cada categoria e desmarcar itens individuais, ou abrir o item no Finder.

## Segurança

- **Mover para a Lixeira** é o modo padrão; o espaço só volta ao esvaziar a Lixeira. "Excluir permanentemente" é opcional e pede confirmação.
- Só remove caminhos **dentro da sua pasta de usuário**, pelo menos dois níveis abaixo dela (ex.: `~/Library/Caches/app`). Nunca toca `/System`, `/Library` nem precisa de `sudo`.
- Links simbólicos são removidos como links; o destino nunca é seguido.
- Caches ligados a iCloud, Casa, Buscar e Safari ficam de fora.
- Feche os apps antes de limpar os caches deles.

Algumas pastas (Mail, backups) exigem **Acesso Total ao Disco**: Ajustes > Privacidade e Segurança > Acesso Total ao Disco > adicione o app (ou o Terminal, se rodar via `cargo run`).

## Instalar com Homebrew

```bash
brew tap 1985epma/systemdata
```

```bash
brew install epma-system-data
```

```bash
epma-system-data
```

## Rodar a partir do código

Requer Rust (`brew install rust` ou [rustup](https://rustup.rs)).

```bash
cargo run --release
```

Relatório no terminal, sem abrir a janela:

```bash
cargo run --release -- --scan
```

Gerar o `.app`:

```bash
./scripts/bundle-macos.sh
```

```bash
open "target/release/EPMA System Data.app"
```

## Estrutura

```
src/
  main.rs        entrada: janela ou --scan
  app.rs         interface egui
  categories.rs  o que é limpo e onde
  scanner.rs     medição em disco (threads em segundo plano)
  cleaner.rs     Lixeira / exclusão + trava de segurança
  util.rs        formatação e espaço em disco
scripts/
  bundle-macos.sh
```

```bash
cargo test
```
