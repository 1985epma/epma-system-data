# Proposta de interface — EPMA System Data

Aplicação dos princípios do `/layout-saas` (hierarquia visual, uma cor de destaque, tabelas densas, estados de loading/vazio/erro, divulgação progressiva) ao app de limpeza, com **tema claro/escuro** e **dois idiomas (pt-BR e en-US)**.

> **Sobre os números:** o disco (237,9 GB de 245,1 GB, 97%), as categorias e os caminhos vêm de um `scan` real feito nesta máquina. Os estados de erro de permissão e pós-limpeza são **ilustrativos** (nesta máquina o Mail não deu erro e nada foi apagado).
> **Sobre a tecnologia:** o app é egui, não web. O `mockup.html` (HTML + Tailwind) é só a referência visual; as seções finais traduzem tudo para egui 0.29 (a versão do projeto).

## Como ver

```
outputs/
  pt-BR/light/  pt-BR/dark/    9 telas cada
  en-US/light/  en-US/dark/    9 telas cada
  mockup.html                  mockup interativo (botões de idioma e de tema na topbar)
```

O mockup abre direto no navegador. Parâmetros: `mockup.html#erro:dark:en-US` (tela : tema : idioma). Telas: `principal`, `carregando`, `expandido`, `erro`, `concluido`, `vazio`, `confirmar`, `permanente`, `compacto`. Sem parâmetros de tema/idioma, ele segue o sistema.

---

## 1. Tela principal

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/01-principal.png" width="480"> | <img src="pt-BR/dark/01-principal.png" width="480"> |

| Hoje | Proposta |
|---|---|
| O valor a liberar só aparece dentro do botão "Limpar X" | Cartão-resumo com **768,0 MB** em destaque: é a primeira coisa que se lê |
| Barra de disco neutra, mesma cor com 50% ou 97% | Cartão do disco com pílula **Crítico · 97%** e barra vermelha, usando os limites do `diag` (90% e 95%) |
| 11 cartões iguais em lista plana | Duas seções: **Seguro** (marcado) e **Revisar** (desmarcado), cada uma com subtotal |
| Seguro/revisar é uma palavra pequena colorida | Pílula suave no cabeçalho da seção, sem repetir em toda linha |
| 8 categorias vazias ocupam espaço | Grupo recolhido "Nada para limpar ou indisponível (8)" |
| Tamanho só como texto | Número alinhado à direita + mini-barra proporcional à maior categoria |
| Botões com o mesmo peso | **Um** botão primário (azul): "Limpar 768,0 MB". "Reescanear" é secundário |
| Radios Lixeira/Permanente sempre visíveis | Seletor "Destino: Lixeira ▾" na barra de ações (divulgação progressiva) |

Decisões de layout:
- **Sidebar** estreita com **Limpeza / Discos / Histórico**. "Discos" expõe o `diag`, hoje só no terminal. Se o app ficar só com a tela de limpeza, a sidebar pode ser omitida.
- **Topbar** com breadcrumb, filtro de categorias (⌘K), status do último scan e os botões de **idioma** e **tema**. Sino e seletor de workspace do padrão SaaS **não foram usados**: não fazem sentido num app local.
- **Cor de destaque única**: azul, só no botão primário, nos checkboxes e no item ativo. Verde, âmbar e vermelho só como semântica de status.

## 2. Carregando (skeleton)

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/02-carregando.png" width="480"> | <img src="pt-BR/dark/02-carregando.png" width="480"> |

Mantém a estrutura da tela enquanto escaneia: nome e caminho já aparecem, só o tamanho vira placeholder. Barra de progresso fina ("Escaneando 4 de 11…"), checkboxes desabilitados e "Limpar" inativo até terminar. Hoje é só um spinner por cartão.

## 3. Categoria expandida e menu de ações

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/03-categoria-expandida.png" width="480"> | <img src="pt-BR/dark/03-categoria-expandida.png" width="480"> |

- Cabeçalho da lista: contagem, **Marcar todos / Nenhum**, ordenação por tamanho.
- Tamanho à direita, caminho em fonte monoespaçada.
- O botão "Finder" repetido em cada linha vira menu **⋯** na última coluna: *Mostrar no Finder*, *Copiar caminho*, *Não incluir na limpeza*.
- "Ver todos os 47 itens" substitui o texto "… e mais N itens menores".

## 4. Erro de permissão (inline, sem pop-up)

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/04-erro-permissao.png" width="480"> | <img src="pt-BR/dark/04-erro-permissao.png" width="480"> |

- Banner vermelho suave no topo com ação direta: **Abrir Ajustes do Sistema** (`x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles`).
- A linha afetada ganha borda e fundo vermelhos suaves com a causa e o que fazer.
- Categoria indisponível (aqui, `simctl` realmente ausente nesta máquina) fica esmaecida, com a nota original.

## 5. Pós-limpeza e painel lateral

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/05-pos-limpeza.png" width="480"> | <img src="pt-BR/dark/05-pos-limpeza.png" width="480"> |

- Banner verde: "768,0 MB movidos para a Lixeira", com **Abrir Lixeira** (o espaço só volta ao esvaziá-la).
- O histórico sai do fim da rolagem e vira um **drawer** com abas *Resumo / Falhas / Registro*, aberto por "Ver detalhes".

## 6. Estado vazio

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/06-estado-vazio.png" width="480"> | <img src="pt-BR/dark/06-estado-vazio.png" width="480"> |

Ilustração de linha, frase de uma linha com o benefício e CTA direto: **Reescanear** (primário) e **Incluir categorias de revisão**.

## 7. Confirmação (Lixeira)

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/07-confirmar-lixeira.png" width="480"> | <img src="pt-BR/dark/07-confirmar-lixeira.png" width="480"> |

Lista cada categoria com seu tamanho e total, aviso azul sobre o destino. O botão diz o que faz: **Mover para a Lixeira**.

## 8. Confirmação (exclusão permanente)

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/08-confirmar-permanente.png" width="480"> | <img src="pt-BR/dark/08-confirmar-permanente.png" width="480"> |

Tom vermelho em toda a cadeia (seletor de destino, botão da barra, modal). O botão só habilita depois de digitar a palavra de confirmação, igual ao CLI (`--permanent` exige `apagar`), então janela e terminal têm o mesmo nível de proteção.

## 9. Janela mínima (680 px)

| Claro | Escuro |
|---|---|
| <img src="pt-BR/light/09-janela-minima.png" width="300"> | <img src="pt-BR/dark/09-janela-minima.png" width="300"> |

Sidebar vira só ícones, os cartões empilham, a descrição das linhas some e a barra de ações encurta. O app já define `with_min_inner_size([680, 480])` em [src/main.rs](../src/main.rs), então essa é a largura real mínima.

---

## Tema claro e escuro

Todas as cores são **tokens** com o mesmo nome nos dois temas. Nada de cor fixa nos componentes.

| Token | Claro | Escuro |
|---|---|---|
| Fundo da janela | `#F8FAFC` | `#0A0A0A` (preto neutro) |
| Cartão / painel | `#FFFFFF` | `#121212` |
| Borda | `#E2E8F0` | `#2A2A2A` |
| Texto | `#1E293B` | `#E5E5E5` |
| Texto secundário | `#64748B` | `#A3A3A3` |
| Destaque (botão, checkbox) | `#2563EB` | `#3B82F6` |
| Destaque em texto/links | `#2563EB` | `#93C5FD` |
| Seguro / Revisar / Erro (fundo suave) | verde, âmbar, vermelho claros | os mesmos matizes em tom escuro translúcido, com texto claro |

Regras que valem nos dois temas:
- O escuro é **preto neutro** (cinzas sem tom azulado). O azul aparece só como destaque.
- Uma única cor de destaque; o azul fica **um tom mais claro no escuro** para manter contraste com o texto branco do botão.
- As cores semânticas mudam de **fundo e texto**, não de significado: vermelho continua sendo "crítico/irreversível".
- A cor do botão primário não muda no hover por "clarear" (no escuro isso estoura): usa um token `accent-hover` próprio.
- Sobreposição (modal e drawer) é preta translúcida nos dois temas.
- Padrão: **seguir o sistema**. O botão ☾/☀ na topbar fixa uma escolha, que fica salva.

## Idioma (pt-BR e en-US)

| Claro | Escuro |
|---|---|
| <img src="en-US/light/01-principal.png" width="480"> | <img src="en-US/dark/01-principal.png" width="480"> |
| <img src="en-US/light/04-erro-permissao.png" width="480"> | <img src="en-US/dark/04-erro-permissao.png" width="480"> |
| <img src="en-US/light/08-confirmar-permanente.png" width="480"> | <img src="en-US/dark/08-confirmar-permanente.png" width="480"> |

As 9 telas existem nos dois idiomas, em `en-US/light/` e `en-US/dark/`.

O que muda além do texto:
- **Número**: `767,8 MB` vira `767.8 MB` (separador decimal do idioma).
- **Plural**: "1 item" / "N items", "1 categoria" / "N categories".
- **Palavra de confirmação**: `apagar` vira `delete`. Mesmo que o app esteja em inglês, o CLI continua aceitando só `apagar`, a menos que ele também seja localizado (ver abaixo).
- **Caminhos, IDs de categoria e flags** não são traduzidos (`~/Library/Caches`, `user_caches`, `--permanent`).
- Botão **PT/EN** na topbar para trocar na hora; padrão = idioma do sistema, com pt-BR como fallback.

> No mockup a tradução é feita sobre o texto já renderizado, só para gerar as telas. No app ela precisa ser uma tabela de strings de verdade (abaixo).

---

## Como traduzir para egui 0.29

| Proposta | Onde / como |
|---|---|
| Paleta e tipografia | `cc.egui_ctx.set_visuals(...)` e `set_style(...)` em `App::new` ([src/app.rs:78](../src/app.rs#L78)); Inter via `set_fonts` (embutir o `.ttf`) |
| Tema claro/escuro | Montar `Visuals::light()` e `Visuals::dark()` a partir dos tokens da tabela acima e aplicar com `ctx.set_visuals_of(Theme::Light, ...)` / `Theme::Dark`; `ctx.set_theme(egui::ThemePreference::System)` faz seguir o sistema (e `Light`/`Dark` fixa a escolha) |
| Cores fora do `Visuals` (verde/âmbar/vermelho, pílulas) | Hoje são `const GREEN/AMBER/RED` em [app.rs:21](../src/app.rs#L21). Trocar por uma `struct Palette { ok_bg, ok_fg, warn_bg, … }` com `fn for_theme(dark: bool)`, escolhida por `ui.visuals().dark_mode` |
| Sidebar | `egui::SidePanel::left("nav").exact_width(224.0)` (56 px quando a janela for estreita) |
| Topbar + status | `TopBottomPanel::top` no lugar do `header` atual ([app.rs:201](../src/app.rs#L201)) |
| Cartão-resumo e disco | `egui::Frame::none().fill(card).stroke(...).rounding(12.0)` + `RichText::size(40.0)`; cor da barra escolhida pelos limites 90/95% (reaproveitar a regra de `diag.rs`) |
| Seções Seguro / Revisar | Ordenar `self.cats` por (risco, tamanho) e desenhar duas listas com subtotal |
| Pílulas | `Frame` com `fill` suave + `RichText::small()` |
| Mini-barra proporcional | `ui.painter().rect_filled(...)` |
| Skeleton | `painter().rect_filled` cinza no lugar do tamanho enquanto `state.scanning` |
| Menu ⋯ | `ui.menu_button("⋯", ...)` ou `response.context_menu(...)` |
| Drawer | `SidePanel::right("detalhes")` condicional |
| Modal | No 0.29 não existe `egui::Modal` (entrou no 0.30): usar `Window` com `anchor` e, por trás, uma `Area` em tela cheia pintando o fundo escurecido e capturando cliques. Ao atualizar o eframe, trocar por `egui::Modal` |
| Confirmação por digitação | `TextEdit` + `add_enabled(texto == t(Key::ConfirmWord), ...)` |
| Abrir Ajustes / Lixeira | `Command::new("open").arg("x-apple.systempreferences:...")` e `open ~/.Trash`, como o `open -R` que já existe |

### Implementando o idioma

1. **Sem dependência nova para começar**: um `src/i18n.rs` com `enum Lang { PtBr, EnUs }`, `enum Key { … }` e `fn t(lang: Lang, key: Key) -> &'static str` com um `match` por idioma. O compilador garante que nenhum texto fique sem tradução. Textos com número usam funções (`items(lang, n)`), para o plural.
2. **Detectar o idioma**: crate `sys-locale` (`sys_locale::get_locale()`); se começar com `en` → `EnUs`, senão `PtBr`. Preferência salva em `eframe::Storage`, editável pelo botão da topbar. Variável `EPMA_LANG=pt-BR|en-US` e `--lang` no CLI.
3. **Número**: hoje `format_bytes` troca `.` por `,` fixo ([src/util.rs:18](../src/util.rs#L18)) e os testes esperam `"1,5 KB"`. Passar o `Lang` (ou o separador) como argumento e manter `pt-BR` nos testes existentes; adicionar casos `en-US`.
4. **CLI**: [tests/cli.rs](../tests/cli.rs) tem ~24 verificações de texto em português (`"Total em arquivos"`, `"Ensaio"` …). Para não quebrar o contrato nem scripts de quem já usa, o CLI deve **continuar em pt-BR quando `LANG` é português ou não definido**, e os testes passam a fixar `EPMA_LANG=pt-BR`. As chaves de `--json` (`kind`, `risk`, nomes de campos) **nunca** são traduzidas.
5. **Cobertura**: um teste que percorre todas as `Key` nos dois idiomas e falha se alguma estiver vazia ou igual nos dois (exceto as de propósito, como `diag`).

## Ordem sugerida de implementação

1. `i18n.rs` + `Palette` por tema + `Visuals` claro/escuro (base de tudo; sem isso o resto vira retrabalho).
2. Cartão-resumo, barra de ações e seletor de destino (telas 1 e 7).
3. Seções Seguro/Revisar, grupo recolhido, skeleton e estado vazio (telas 1, 2, 6).
4. Banner de erro com ação e lista expandida com menu ⋯ (telas 3 e 4).
5. Drawer de pós-limpeza e confirmação por digitação (telas 5 e 8).
6. Sidebar com a aba "Discos" (exige expor o `diag` na janela).

## Arquivos

- `mockup.html`: fonte do mockup. Carrega Tailwind e a fonte Inter pela internet.
- `pt-BR/` e `en-US/`, cada um com `light/` e `dark/`: 9 capturas a 2x (1100×820; a janela mínima é 680×720).
