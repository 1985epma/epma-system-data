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
| Safari: caches | `~/Library/Caches/com.apple.Safari`, ícones em `~/Library/Safari` | marcado |
| Safari: histórico e downloads | `~/Library/Safari/History.db`, `Downloads.plist`, abas recentes | revisar |
| Safari: cookies e dados de sites | `Cookies.binarycookies`, `LocalStorage`, `WebsiteData` | revisar |
| Safari: dados de extensões | `WebExtensions`, `AppExtensions`, `Extensions` | revisar |
| Edge: caches | `Cache`, `Code Cache`, `GPUCache`, Service Workers (todos os perfis) | marcado |
| Edge: histórico e downloads | `History`, `Top Sites`, `Favicons`, `Sessions` (todos os perfis) | revisar |
| Edge: cookies e dados de sites | `Network/Cookies`, `Local Storage`, `IndexedDB` (todos os perfis) | revisar |
| Edge: extensões e seus dados | `Extensions`, `Local Extension Settings` (todos os perfis) | revisar |

### Navegadores (Safari e Edge)

Para limpar tudo de navegador de uma vez: na janela, **Marcar navegadores**; no terminal, `--only browsers`.

```bash
epma-system-data clean --only browsers --dry-run
```

```bash
epma-system-data clean --only browsers
```

Só entram dados de limpeza. Senhas (`Login Data`), favoritos, preferências, preenchimento automático e o `Local State` do Edge nunca são tocados; para remover um perfil inteiro use `edge://settings/profiles`. Cookies e dados de sites desconectam você dos sites. Feche o Safari e o Edge antes, e conceda Acesso Total ao Disco para o Safari.

Categorias "revisar" começam desmarcadas. Dá para expandir cada categoria e desmarcar itens individuais, ou abrir o item no Finder.

## Segurança

- **Mover para a Lixeira** é o modo padrão; o espaço só volta ao esvaziar a Lixeira. "Excluir permanentemente" é opcional e pede confirmação.
- Só remove caminhos **dentro da sua pasta de usuário**, pelo menos dois níveis abaixo dela (ex.: `~/Library/Caches/app`). Nunca toca `/System`, `/Library` nem precisa de `sudo`.
- Links simbólicos são removidos como links; o destino nunca é seguido.
- A trava confere o caminho **real**, não só o texto: se uma pasta de categoria (por exemplo `~/Library/Caches`) for um link para fora da sua pasta de usuário, ela é ignorada com um aviso e nada é apagado lá. Isso vale também para quem moveu o cache para um disco externo por link simbólico: nesse caso, limpe pelo próprio disco.
- Só uma limpeza roda por vez no seu usuário (janela, terminal e agendamento dividem a mesma trava). A segunda recusa com "outra limpeza está em andamento" em vez de disputar os mesmos arquivos.
- Tamanhos: arquivos com vários hard links contam uma vez só. Clones do APFS não aparecem para o sistema de arquivos e continuam somando por inteiro, então o número pode ficar acima do espaço que volta de fato.
- Caches ligados a iCloud, Casa e Buscar ficam de fora. O cache do Safari sai só pela categoria própria dele.
- Feche os apps antes de limpar os caches deles.

Algumas pastas (Mail, backups) exigem **Acesso Total ao Disco**: Ajustes > Privacidade e Segurança > Acesso Total ao Disco > adicione o app (ou o Terminal, se rodar via `cargo run`).

## Instalar com Homebrew

```bash
brew tap 1985epma/systemdata
```

Nas versões recentes do Homebrew, fórmulas de um tap de terceiros só carregam depois de você confiar nele (sem isso aparece `Refusing to load formula ... from untrusted tap`):

```bash
brew trust 1985epma/systemdata
```

```bash
brew install epma-system-data
```

```bash
epma-system-data
```

Para atualizar: `brew update && brew upgrade epma-system-data`.

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

## A janela

Sem argumentos o app abre a janela:

- **Limpeza**: um cartão mostra quanto dá para liberar agora e outro, o uso do disco (aviso a 90%, crítico a 95%, os mesmos limites do `diag`). As categorias aparecem em **Seguro** (marcadas) e **Revisar** (desmarcadas), da maior para a menor; as vazias ficam num grupo recolhido. Clique numa categoria para ver os itens, desmarcar um por um ou abrir no Finder pelo menu `•••`.
- **Discos**: o mesmo diagnóstico do `diag` (discos físicos, SMART e volumes), somente leitura.
- **Histórico**: o que cada limpeza da sessão fez, com as falhas.
- **Ajustes**: idioma e tema.
- Confirmação antes de limpar. **Exclusão permanente** exige digitar a palavra de confirmação (`apagar` em pt-BR, `delete` em en-US).
- Erros aparecem no próprio lugar, nunca em pop-up; o de permissão tem um botão que abre os Ajustes do Sistema.
- Atalhos: **⌘K** foca o filtro de categorias e **⌘R** reescaneia.

**Tema e idioma.** O tema (claro, escuro ou automático) e o idioma (pt-BR, en-US ou automático) seguem o sistema por padrão; os botões na barra superior alternam na hora e **Ajustes** volta ao automático. A escolha fica em `~/Library/Application Support/epma-system-data/prefs`. O destino da limpeza (Lixeira ou permanente) nunca é lembrado: a janela sempre abre em Lixeira. A linha de comando continua em português.

A fonte é a [Inter](https://rsms.me/inter/) (licença OFL, em `assets/fonts/`).

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
| (só no terminal) Diagnóstico de discos | `epma-system-data diag` |

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

Outras opções: `--json` (saída estável para scripts, em `list`, `scan`, `disk`, `diag` e `clean`; no `clean`, cada falha traz `kind`: `permission_denied`, `not_found`, `outside_home` ou `other`), `--quiet`, `--no-color` (também vale `NO_COLOR`) e `epma-system-data completions zsh|bash|fish` para autocompletar. `--scan` continua funcionando como alias de `scan`.

Segurança no terminal:

- `clean` sempre mostra o plano antes. Sem terminal interativo e sem `--yes`, ele recusa e sai com código 3, para um agendamento nunca apagar por engano.
- Categorias "revisar" só entram com `--only` ou `--include-review`.
- Com `--permanent`, a confirmação interativa exige digitar `apagar`.
- A mesma trava da janela vale aqui: nada fora da sua pasta de usuário é tocado.

| Código | Significado |
|---|---|
| 0 | Sucesso (inclui ensaio e "nada para limpar") |
| 1 | Limpeza com falhas parciais (ex.: falta Acesso Total ao Disco), outra limpeza em andamento, ou `diag` com achado crítico |
| 2 | Uso incorreto (flag ou id de categoria inválido) |
| 3 | Recusado: pediria confirmação, mas não há terminal e faltou `--yes` |

## Diagnóstico de discos (macOS e Linux)

```bash
epma-system-data diag
```

Somente leitura: não altera nada. Mostra os discos físicos, os volumes e uma lista de achados.

| Parte | macOS | Linux |
|---|---|---|
| Discos físicos (modelo, tamanho, SSD ou HDD, interno ou externo) | `diskutil` | `/sys/block` |
| Saúde SMART | `diskutil info` (já vem no sistema) | `smartctl -H` do smartmontools, **precisa de `sudo`** |
| Volumes (espaço, inodes, somente leitura) | `getmntinfo` | `/proc/self/mounts` e `statvfs` |

| Achado | Nível | Regra |
|---|---|---|
| Volume cheio | aviso a partir de 90%, crítico a partir de 95% | Calculado como o `df`. Volumes somente leitura (ISO, imagem de disco) e volumes APFS do mesmo container não repetem o aviso |
| Inodes quase esgotados | aviso 90%, crítico 95% | Dá para ficar sem criar arquivos mesmo com espaço livre |
| Montado somente leitura | aviso | Só no Linux: lá costuma indicar erro no sistema de arquivos. No macOS é normal (raiz selada, imagens de disco) |
| SMART reporta falha | crítico | Faça backup e planeje trocar o disco |
| SMART não lido | info | Falta `smartctl` ou permissão; a nota diz o que fazer |

Com achado crítico o código de saída é 1, então dá para usar em agendamento. `--json` traz os mesmos dados. No Linux, instale `smartmontools` (`apt install smartmontools`) e rode `sudo epma-system-data diag` para ver o SMART.

## Estrutura

```
src/
  main.rs        entrada: janela ou CLI
  cli.rs         comandos de terminal (clap)
  diag.rs        diagnóstico de discos (macOS e Linux)
  lock.rs        trava: uma limpeza por vez
  plan.rs        seleção e relatório compartilhados pela janela e pelo CLI
  app.rs         janela egui: telas, estados e confirmação
  theme.rs       paleta claro/escuro, fonte e Visuals do egui
  widgets.rs     componentes (cartão, pílula, checkbox, banner, skeleton)
  i18n.rs        textos em pt-BR e en-US (só a janela)
  prefs.rs       idioma e tema escolhidos, guardados em arquivo
  categories.rs  o que é limpo e onde
  scanner.rs     medição em disco (threads em segundo plano)
  cleaner.rs     Lixeira / exclusão + trava de segurança
  util.rs        formatação e espaço em disco
assets/
  fonts/         Inter (Regular e SemiBold) e a licença
scripts/
  bundle-macos.sh
tests/
  cli.rs         contrato do CLI (códigos de saída, dry-run, segurança)
.github/
  workflows/ci.yml   formatação, clippy, testes (macOS e Linux) e auditoria
  dependabot.yml     atualizações semanais de crates e ações
deny.toml        licenças permitidas e avisos de segurança (cargo-deny)
Dockerfile.test  Linux para `make test-linux`
Makefile         os mesmos comandos do CI
```

## Desenvolvimento

```bash
make check
```

Roda o que o CI roda: `fmt-check`, `lint` (clippy sem avisos), `test` e `deny`. Cada etapa também existe separada (`make help` lista todas).

| Alvo | O que faz |
|---|---|
| `make fmt` | Formata o código (`cargo fmt`) |
| `make lint` | Clippy com avisos tratados como erro |
| `make test` | Testes na máquina atual |
| `make test-linux` | Testes em Linux dentro do Docker (precisa do Docker) |
| `make deny` | Licenças e avisos de segurança das dependências (`cargo install cargo-deny`) |

O CI (`.github/workflows/ci.yml`) roda em cada push e PR: formatação, clippy e testes em macOS (Apple Silicon e Intel) e Linux, mais a auditoria de dependências, que também roda toda segunda-feira para pegar avisos novos sem precisar de commit. As ações do GitHub estão fixadas por SHA e o Dependabot propõe as atualizações.

### Ver cada estado da janela

Em build de **debug**, `EPMA_PREVIEW` abre a janela direto num estado, e `EPMA_WINDOW=680x560` define o tamanho. Com `EPMA_PREVIEW` definida a janela **nunca executa limpeza**; mesmo assim, aponte `HOME` para uma pasta de teste para não escanear os seus arquivos.

```bash
HOME=/tmp/casa-de-teste EPMA_PREVIEW=confirm,permanent cargo run
```

Valores (separados por vírgula): `disks`, `history`, `settings`, `confirm`, `permanent`, `details`, `denied`, `done`, `fail`, `busy`, `empty`, `expand:ID`, `filter:texto`. No release a variável é ignorada.

Avisos de segurança aceitos de propósito ficam em `deny.toml`, cada um com o motivo. Hoje são dois do `quick-xml` 0.30, que só entra no Linux pelo `eframe`; saem quando o `eframe` for atualizado.
