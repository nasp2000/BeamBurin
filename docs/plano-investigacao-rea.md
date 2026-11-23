# Plano de investigação com REA, rust-anything e craft-anything

Estado: plano aprovado para execução por lotes. Este documento **não contém
código** e não altera comportamento: define o que falta cobrir face à
referência, que ferramenta responde a cada pergunta, os comandos, a ordem e o
que corre no Raspberry Pi 4 e o que exige o worker Windows.

Regra de nomenclatura: o artefacto analisado é sempre **«a referência»**. O
teste `tests/no_product_names.rs` falha se o nome do produto, caminhos com esse
nome ou nomes de pasta aparecerem em `docs/`, `src/`, `tests/`, `scripts/` ou
no histórico git. Os caminhos reais do artefacto vivem nas notas externas
(`~/Code/beamburin-notes/` e `~/Code/rea-work/`), nunca aqui.

Regra de âmbito: a referência é uma aplicação comercial de preparação de laser
com controlo de máquina, câmara, rotativo e galvo. O BeamBurin prepara o
trabalho e entrega um ficheiro; tudo o que seja ligar, enviar, fazer homing ou
configurar controlador está fora (AGENTS.md, PLAN §1.3). Só se investiga o que
cai dentro do âmbito: desenho, operações, parâmetros, imagens, pré-visualização
e exportação.

Método clean room: observa-se o artefacto entregue, escreve-se primeiro o
comportamento esperado em notas externas, implementa-se de raiz em Rust com
egui. Não se copia código, texto, ícone, layout nem recurso do artefacto.

---

## 1. Base de partida (medida em 2026-11-23)

| Facto | Valor medido |
|---|---|
| Artefacto da referência | PE32+ x86-64, 23 566 248 B, `sha256 a3fe810e…c9368d` |
| Símbolos privados | PDB de 17 715 200 B, ao lado do executável |
| Versão declarada | 2.0.02 no ChangeLog incluído (confirmar no diálogo About em runtime) |
| Runtime | Qt 5 no diretório (`Qt5Core/Gui/Widgets/…`), 45 DLLs, 76 entradas |
| Traduções | 25 ficheiros `.qm` (~18 MB) — catálogo de texto de UI mais barato que existe |
| BeamBurin | 98 362 linhas Rust, versão 0.1.0, `master`, CI como única verificação |
| Já portado (T32) | 7 itens: janelas alinhadas, painel redimensionável, diálogos de ficheiro, zoom gradual, *vectorize* + *trace centerline*, seleção sem linha dupla, *Adjust Image* |
| REA | CLI 6.8.0 em `~/.local/bin/rea`, MCP com 229 ferramentas, Ghidra 12.1.4 + JDK 25 no Pi |
| craft-anything | MCP com toolsets `core` (37), `native` (55), `formats` (36), `runtime` (14), `application` (17) |
| rust-anything | porta TS/JS, Python, C, Go, Ruby, Java, Kotlin → Rust; **não lê C++ nem binários** |
| Pi 4B | 4 núcleos, 7 GB (≈2,8 GB livres), Debian 13 aarch64 |
| Worker Windows | `100.78.156.123`, Tailscale «relay mad; offline, last seen 8h ago», SSH em timeout → **pendente** |

Consequência imediata: a referência é C++/Qt, portanto o rust-anything não se
aplica ao artefacto; só entra quando houver uma implementação de referência
numa linguagem suportada (ver §3.3). Toda a análise do binário é REA +
craft-anything.

---

## 2. (a) Cobertura: o que falta, por prioridade

Ponto de partida confirmado por leitura do código (`grep` em `src/`, 0
ocorrências = ausente) e por cadeias do artefacto (extraídas das traduções,
guardadas fora do repo). A varredura F1/F2 fecha a lista; estes itens são os
que já se sabe que faltam e servem para dimensionar os lotes.

### P0 — corta mal o trabalho ou falta controlo de qualidade

| ID | Área | O que a referência tem | Estado no BeamBurin | Onde investigar |
|---|---|---|---|---|
| G-01 | Contornos fechados | **Sobrecorte (%)**: na última passagem, continua a cortar para além do ponto final | Ausente: `grep overcut src/` = 0 | REA estático (campo no diálogo de operação) + runtime para medir a fórmula |
| G-02 | Potência | **Potência mínima e máxima** com limitação por operação | Ausente: `min_power`/`max_power` = 0; só `power` e `constant_power` | REA + runtime no worker (ver quantização de S) |
| G-03 | Corte | **Modo de perfuração**: alterna cortar/não cortar (corte tracejado) | Ausente: `perforat` = 0 | REA (parâmetros) + runtime (espaçamento real no G-code) |
| G-04 | Ordem | **Prioridade por objeto** («desenhar este objeto primeiro») e diálogo de **otimização de corte** (ordem, ida e volta, viagens) | Parcial: `order_by_operation`/`assisted_ordering` (vizinho mais próximo); sem prioridade por objeto e sem opções expostas | REA + runtime (comparar ordem de saída em desenho de teste) |

### P1 — funcionalidade visível em falta

| ID | Área | O que a referência tem | Estado no BeamBurin | Onde investigar |
|---|---|---|---|---|
| G-05 | Biblioteca | **Biblioteca de artes**: formas reutilizáveis, formato próprio, arrastar entre bibliotecas | Ausente: não há módulo de biblioteca de formas (só perfis/materiais) | REA (formato e fluxo) + craft-anything `formats` |
| G-06 | Duplicação | **Copiar ao longo de um caminho** | Ausente; só arrays em linha e círculo (`arrays.rs`) | REA (parâmetros: nº, orientação, «selecionar resultado») |
| G-07 | Imagem | **Trace de vários ficheiros** com as mesmas definições | Ausente; trace é por imagem (`dialogs_trace.rs`) | REA (lista de opções) + runtime |
| G-08 | Texto | **Wobble** (aspeto desenhado à mão) | Ausente: `wobble` = 0 | REA (parâmetros) |
| G-09 | Gravação | **Comprimento da rampa** (subida/descida de potência) | Ausente: `ramp` só em comentário do importador DXF | REA + runtime (perfil de S no G-code) |
| G-10 | Preenchimento | **Ângulo por passagem** além do cruzado | Parcial: `cross_hatch` booleano, ângulo único | REA (diálogo de operação) |
| G-11 | Nós | **Barra de edição de nós** contextual | Ausente (`node_toolbar` = 0); nós só por menu/painel | craft-anything native (captura da barra) |

### P2 — decidir antes de investigar

| ID | Área | Nota |
|---|---|---|
| G-12 | *3D slice* | Funcionalidade de desenho 3D; fora do âmbito 2D de preparação. Decidir com o Nuno antes de gastar horas de REA. |
| G-13 | Código de barras | Só Code 39 por decisão registada no PLAN §33; outras simbologias exigem ordem nova. |
| G-14 | Texto variável/CSV | Excluído por decisão do PLAN §33. |
| G-15 | Altura de material, Z por passagem, laser 2, rotativo, galvo, câmara | Controlo de máquina: fora de âmbito, não se investiga. |

### O que a varredura ainda tem de responder (desconhecidos)

1. Lista completa de controlos por diálogo que a referência tem e nós não —
   sobretudo em *cut settings*, imagem, trace, offset, arrays, warp e pré-visualização.
2. Valores por omissão e limites (intervalos, tolerâncias, percentagens) de cada
   parâmetro que já existe nos dois lados mas com gama diferente.
3. Semântica do G-code produzido (ordem, S, F, ar, tabs) em desenhos de teste
   iguais nos dois programas.
4. Diferenças de importação (DXF com texto, splines, SVG com máscaras) e de
   exportação (SVG, DXF, PDF, PNG) para os mesmos fixtures.
5. Candidatos vistos no catálogo de texto mas ainda sem controlo confirmado:
   passagem direta de um objeto sem processamento, percentagem de sobreposição
   do preenchimento, ajuste fino do deslocamento do varrimento. Entram na lista
   final só depois de observados num diálogo.

---

## 3. (b) Por ferramenta: alvos, comandos e ordem

### 3.1 REA — análise do artefacto (a espinha dorsal)

O REA faz três coisas que interessam: identidade e recursos do PE, catálogo de
cadeias/símbolos, e decompilação com Ghidra. A primeira e a segunda são baratas
e correm no Pi; a terceira é pesada (ver §4).

Preparação (uma vez por sessão):

```bash
export REF_DIR="$HOME/Code/rea-work/<pasta-do-artefacto>"   # nome real só nas notas externas
export REF_EXE="$REF_DIR/<executavel>.exe"
export GHIDRA_INSTALL_DIR="$HOME/tools/ghidra_12.1.4_PUBLIC"
export JAVA_HOME="/usr/lib/jvm/java-25-openjdk-arm64"
rea doctor
```

Alvos e comandos, por ordem de custo crescente:

1. **Identidade e recursos** (segundos, Pi):
   `rea inspect-pe-resources "$REF_EXE" --format json` — árvore de recursos,
   ícones, versão. Já executado: `sha256 a3fe810e…`, 23 566 248 B.
2. **Layout e importações** (segundos, Pi):
   `rea inspect-pe-imports "$REF_EXE"` e
   `rea inspect-desktop-application "$REF_DIR"` (identifica runtime e DLLs; é
   este passo que confirma Qt e que separa UI de máquina).
3. **Catálogo de texto** (minutos, Pi): as 25 traduções são o inventário de
   todos os rótulos, tooltips e diálogos. Decodificação já validada (13 656
   cadeias em pt):
   ```bash
   python3 - "$REF_DIR/…_pt.qm" <<'PY'
   import re, sys
   data = open(sys.argv[1], "rb").read()
   text = data.decode("utf-16-be", "ignore")
   runs = [r.strip() for r in re.findall(r"[^\x00-\x08\x0b-\x1f]{4,}", text)]
   open("/tmp/opencode/t38/ref-pt.txt", "w").write("\n".join(runs))
   PY
   ```
   (o caminho do `.qm` entra por variável, não fica escrito no repo)
4. **Símbolos do PDB** (minutos, Pi): importar o PDB no Ghidra antes da
   análise dá nomes de classes e métodos — é o que transforma «função
   anónima» em «diálogo de operação». Ferramentas: `llvm-pdbutil` ou `pdbparse`
   num venv, exportar mapa `endereço → símbolo` e alimentar o projeto Ghidra.
5. **Panorama e busca** (análise média, worker): `rea analyze "$REF_EXE"`,
   depois `rea search "$REF_EXE" "<literal>"` para strings e nomes de
   procedimento, `rea inspect "$REF_EXE"` para o panorama. No MCP:
   `open_binary`, `binary_overview`, `list_strings`/`search_strings`,
   `list_procedures`.
6. **Funções e fluxo** (análise pesada, worker):
   `rea function "$REF_EXE" --procedure "<símbolo>"`,
   `rea decompile "$REF_EXE" --procedure "<símbolo>"`,
   `rea xrefs "$REF_EXE" --address "<endereço|símbolo>"`,
   `rea trace "$REF_EXE" "<literal>"`.
   No MCP: `analyze_function`, `batch_decompile`, `read_function_instructions`,
   `xrefs`, `trace_feature`. Uma função por diálogo confirmado — não se
   decompila «tudo para ver».
7. **Evidência**: `rea evidence-export <bundle.json> <saida.json>` de cada lote
   para `~/Code/rea-work/<pasta>/evidencia/` (fora do repo). O resumo neutro
   entra neste repositório.

Regra de ouro: cada item G-xx fecha com (i) cadeia ou diálogo observado,
(ii) função que o trata (endereço + símbolo), (iii) comportamento descrito em
notas externas, (iv) critério de aceitação para o BeamBurin. Sem os quatro, o
item continua desconhecido.

### 3.2 craft-anything — memória, captura e comparação

Serve para três coisas que o REA não faz: manter o raciocínio (brief, plano,
hipóteses, achados, desconhecidos), capturar UI nativa e comparar o nosso
resultado com o da referência.

Fluxo por lote:

1. `core_session_show` e `core_investigation_show` — retomar o estado.
2. `core_brief_write` (pergunta, alvos, restrições, o que conta como feito).
3. `core_plan_update` (passos `todo/doing/done`).
4. `core_targets_list` — ver o que está aberto; abrir o artefacto pela ferramenta
   de abertura do servidor (o conjunto exposto depende do perfil da ligação) e
   confirmar com `core_session_show`; `list_toolsets` → `enable_toolset`
   `native` e `formats` (e `runtime` quando houver processo).
5. `list_strings`/`analyze_function` equivalentes ficam no REA; aqui registam-se
   **achados** com `core_finding_record` (exige ids de evidência) e
   **desconhecidos** com `core_unknown_add`.
6. `core_report_write` + `core_evidence_export` para
   `~/Code/rea-work/<pasta>/` — nunca para o repo.
7. `core_handoff_write` no fim de cada lote.

Uso concreto:

- **UI da referência** (worker): abrir o diálogo em causa, capturar a árvore de
  acessibilidade e o screenshot, listar controlos, medir limites. O toolset
  `native` do craft-anything é o caminho quando o servidor corre onde a app
  corre; se não correr, o fallback é UIA/AutoHotkey + capturas no worker (ver
  §4) e os ficheiros entram no craft-anything como evidência importada.
- **Formatos**: comparar exportações para o mesmo desenho (SVG, DXF, PDF, PNG)
  com o toolset `formats`; as diferenças viram critérios de aceitação.
- **Rebuild check**: depois de implementado, `rebuild_check` compara o nosso
  build com o comportamento registado, com os mesmos casos.

### 3.3 rust-anything — só quando houver código-fonte de referência

O rust-anything não lê C++ nem binários: `analyze` aceita TS/JS, Python, C, Go,
Ruby, Java e Kotlin. Portanto:

- **Não se usa** para o artefacto. Usá-lo aqui seria começar por inventar uma
  fonte que não existe.
- **Usa-se** quando um item exigir um algoritmo com implementação de referência
  numa linguagem suportada (ex.: *trace*/potrace em C, *dither* numa lib
  Python, *wobble* num motor de desenho). Nesse caso o ciclo é
  `analyze → map → bridge → record → diff → gate`, com `profile`/`bench` para
  provar que o port não perde velocidade, e `shadow` para comparação lado a
  lado. Nunca se marca um port como feito antes do `gate` (fmt, clippy -D
  warnings, testes, sem `unsafe` sem justificação).
- **Não se usa** para reescrever módulos que já são Rust nosso: o BeamBurin não
  é um port e não ganha nada com um `bridge` napi/wasm por unidade.

Critério: se a pergunta for «como é que a referência faz X?», a ferramenta é
REA/craft-anything; se for «como porto esta implementação aberta de X para
Rust?», a ferramenta é rust-anything.

---

## 4. (c) RPi4 vs worker Windows

| Trabalho | Onde | Porquê |
|---|---|---|
| Recursos, layout, importações do PE | Pi | segundos, sem provider |
| Catálogo de texto das traduções + normalização | Pi | CPU leve, resultado é texto |
| Diff catálogo ↔ `src/` do BeamBurin | Pi | `grep`/script, sem análise |
| Escrever especificação neutra e critérios | Pi | trabalho de escrita |
| Importar PDB e mapear símbolos | Pi (tentar) / worker (se doer) | `llvm-pdbutil` é leve; o Ghidra não |
| Análise Ghidra completa (23 MB, Qt, PDB) | **Worker** | 16 GB e x64; no Pi são 2,8 GB livres para um binário grande |
| `decompile-all` e dossiers de funções | **Worker** | horas de CPU; no Pi bloqueia o resto |
| UI real da referência (diálogos, limites, capturas) | **Worker** | a app é Windows; no Pi só haveria Wine, que não é prova |
| Comparar G-code/SVG/DXF exportados pela referência | **Worker** | precisa da app a correr |
| Correr o artefacto CI do BeamBurin e validar | Pi | teste, não build |

Worker: `HP-ProDesk-405-G4`, `ssh -i ~/.ssh/winbox_ed25519 Nasp@100.78.156.123`
(runbook em `~/Code/FreePlots-parity/WINDOWS-WORKER.md`). Estado em
2026-11-23: Tailscale «relay mad; offline, last seen 8h ago», `nc` para a porta
22 em timeout. **Pendente, não bloqueia**: o lote F1a/F2 corre só no Pi; quando
o worker voltar, F1b/F3 arrancam sem repetir o que já está feito.

Boas práticas no worker:

- Copiar o artefacto para `C:\work\ref\` com hash verificado; nada de analisar
  a partir de unidade de rede.
- Análise Ghidra uma vez, exportar o projeto e copiar de volta para o Pi: as
  consultas seguintes (`open_binary` sobre a base de análise) são baratas e não
  repetem o trabalho.
- Registar sempre a versão usada; um artefacto novo invalida os endereços.

---

## 5. Ordem de execução

| Lote | Onde | Duração | Entregável |
|---|---|---|---|
| **F0** Congelar o alvo | Pi | 30 min | Hash do executável e do PDB, `REF_DIR` nas notas externas, `rea doctor` limpo, catálogo de texto das traduções em `/tmp` |
| **F1a** Inventário estático leve | Pi | 1–2 h | Tabela diálogo → cadeias → controlos prováveis; lista de funções candidatas por nome (PDB) |
| **F1b** Análise profunda | Worker | 2–4 h | Projeto Ghidra + PDB, panorama, `list_procedures` filtrado por classes de diálogo, dossiers das funções dos itens P0 |
| **F2** Diff e prioridades | Pi | 1–2 h | Lista final G-xx com estado, evidência e critério de aceitação; notas externas atualizadas |
| **F3** Observação de runtime | Worker | 2–3 h | Capturas e medições dos itens P0/P1 (sobrecorte, perfuração, potência min/máx, ordem, biblioteca, trace múltiplo) |
| **F4** Especificação | Pi | 1 h por item | Comportamento esperado, limites, erros, testes previstos — em notas externas |
| **F5** Implementação | Pi (código) + CI | por lote de 3–5 itens | Código só com ordem nova do Nuno, testes em CI, sem build local |
| **F6** Verificação | Pi | por lote | Artefacto CI a correr no Pi, comparado com a especificação |

Porta de saída de cada lote: lista de itens fechados com evidência, itens que
passaram a desconhecidos e o próximo lote escrito. Nada avança «porque parece
igual».

---

## 6. Critérios de conclusão desta investigação

1. Cada item G-xx tem estado explícito: **em falta**, **parcial**, **igual** ou
   **fora de âmbito**, com a evidência que o sustenta.
2. Cada item em falta tem especificação neutra (comportamento, parâmetros,
   limites, erros) e um critério de aceitação testável em CI.
3. As notas externas têm o catálogo completo e os bundles de evidência; o
   repositório tem apenas este plano e, mais tarde, o resumo neutro.
4. Nenhum ficheiro do repositório nomeia o produto, nenhum código foi escrito
   sem ordem, `target/` não existe no fim de cada lote.

## 7. Riscos e mitigação

| Risco | Mitigação |
|---|---|
| Ghidra no Pi sem memória | análise completa só no worker; projeto copiado para o Pi para consultas |
| PDB sem símbolos privados | cair para cadeias + RTTI/Qt metaobject; registar como desconhecido, não inventar |
| Worker desligado | F0–F2 correm no Pi; F1b/F3 ficam pendentes com o comando pronto |
| Versão do artefacto diferente da esperada | confirmar no About em runtime e congelar hash antes de comparar |
| Contaminação clean room | só comportamento observado; nada de código, texto, ícone ou layout copiado; nome do produto fora do repo |
| Deriva de prioridades | P0 primeiro; P2 só com decisão do Nuno |
