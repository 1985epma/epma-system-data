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

Gerar o `.app`:

```bash
./scripts/bundle-macos.sh
```

```bash
open "target/release/EPMA System Data.app"
```

## Linha de comando

Tudo o que a janela faz também roda no terminal. Sem argumentos abre a janela; com um subcomando, não abre nada.

| Na janela | No terminal |
|---|---|
| Barra de uso do disco | `epma-system-data disk` |
| Cartões de categoria (risco, descrição) | `epma-system-data list` |
| Tamanho por categoria e lista de itens | `epma-system-data scan [--details] [--only ID]` |
| Marcar categorias (padrão: só as "seguro") | `epma-system-data clean [--only ID] [--include-review] [--exclude ID]` |
| Desmarcar um item | `epma-system-data clean --skip CAMINHO` |
| Lixeira ou exclusão permanente | `epma-system-data clean [--permanent]` |
| Janela "Confirmar limpeza" | plano mostrado antes, pergunta `[s/N]` (`--yes` pula) |
| Histórico | relatório por categoria ao fim do `clean` |
| Botão "Finder" | `epma-system-data reveal CAMINHO` |

Dica: rode `list` para ver os `ID`s das categorias.

```bash
epma-system-data scan --details
```

Ensaio primeiro, sem alterar nada:

```bash
epma-system-data clean --dry-run
```

Limpar o que vem marcado por padrão (Lixeira, com confirmação):

```bash
epma-system-data clean
```

Limpar só os backups de iPhone/iPad, de vez:

```bash
epma-system-data clean --only ios_backups --permanent
```

Outras opções: `--json` (saída estável para scripts, em `list`, `scan`, `disk` e `clean`), `--quiet`, `--no-color` (também vale `NO_COLOR`) e `epma-system-data completions zsh|bash|fish` para autocompletar. `--scan` continua funcionando como alias de `scan`.

Segurança no terminal:

- `clean` sempre mostra o plano antes. Sem terminal interativo e sem `--yes`, ele recusa e sai com código 3, para um agendamento nunca apagar por engano.
- Categorias "revisar" só entram com `--only` ou `--include-review`.
- Com `--permanent`, a confirmação interativa exige digitar `apagar`.
- A mesma trava da janela vale aqui: nada fora da sua pasta de usuário é tocado.

| Código | Significado |
|---|---|
| 0 | Sucesso (inclui ensaio e "nada para limpar") |
| 1 | Limpeza com falhas parciais (ex.: falta Acesso Total ao Disco) |
| 2 | Uso incorreto (flag ou id de categoria inválido) |
| 3 | Recusado: pediria confirmação, mas não há terminal e faltou `--yes` |

## Estrutura

```
src/
  main.rs        entrada: janela ou CLI
  cli.rs         comandos de terminal (clap)
  plan.rs        seleção e relatório compartilhados pela janela e pelo CLI
  app.rs         interface egui
  categories.rs  o que é limpo e onde
  scanner.rs     medição em disco (threads em segundo plano)
  cleaner.rs     Lixeira / exclusão + trava de segurança
  util.rs        formatação e espaço em disco
scripts/
  bundle-macos.sh
tests/
  cli.rs         contrato do CLI (códigos de saída, dry-run, segurança)
```

```bash
cargo test
```
