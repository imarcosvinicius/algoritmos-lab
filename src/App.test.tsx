import { useState } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import {
  tryAutoCloseAlgBlock,
  ALG_COMPLETIONS,
  algParser,
} from "./editor/algoritmosLanguage";
import { loadSettings, SETTINGS_STORAGE_KEY } from "./hooks/useIdeSettings";
import { MAX_CONSOLE_CHARS, MAX_CONSOLE_CHUNKS } from "./hooks/useExecutionSession";
import type { IdeBridge } from "./services/tauriBridge";
import type { SessionEventEnvelope } from "./types/ide";
import {
  getContrastRatio,
  isDarkHex,
  isValidHexColor,
  resolveConsoleColors,
} from "./utils/colorUtils";
import { getFileName, getParentDir } from "./utils/pathUtils";
import {
  getRecoverySource,
  RECOVERY_STORAGE_KEY,
  setRecoverySource,
} from "./utils/recoveryDraft";
import {
  normalizeLineEndings,
  normalizeAlgWord,
  suggestFileName,
  usesArquivoCommand,
} from "./utils/algoritmosText";
import { undo } from "@codemirror/commands";
import { StringStream } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { tauriIdeBridge } from "./services/tauriBridge";

function createMockBridge() {
  let listener: ((env: SessionEventEnvelope) => void) | null = null;
  let lastRunId = "run-mock";

  const bridge: IdeBridge = {
    isDesktop: () => true,
    getCoreVersion: vi.fn(async () => "0.2.0"),
    pickOpenAlgPath: vi.fn(async () => "/tmp/teste.alg"),
    pickSaveAlgPath: vi.fn(async () => "/tmp/salvo.alg"),
    pickWorkingDir: vi.fn(async () => "/tmp/trabalho"),
    openAlgFile: vi.fn(async (path: string) => ({
      path,
      name: "teste.alg",
      text: 'algoritmo "teste"\ninicio\n   escreval("Oi")\nfimalgoritmo\n',
      encoding: "windows1252" as const,
    })),
    saveAlgFile: vi.fn(async () => {}),
    confirmAction: vi.fn(async () => true),
    setWindowTitle: vi.fn(async () => {}),
    onWindowCloseRequested: vi.fn(async () => () => {}),
    startExecution: vi.fn(async ({ runId }) => {
      lastRunId = runId;
      return runId;
    }),
    sendInput: vi.fn(async () => {}),
    debugAction: vi.fn(async () => {}),
    updateBreakpoints: vi.fn(async () => {}),
    stopExecution: vi.fn(async () => {}),
    listenSessionEvents: vi.fn(async (fn) => {
      listener = fn;
      return () => {
        listener = null;
      };
    }),
  };

  return {
    bridge,
    emit(event: SessionEventEnvelope["event"], runId?: string) {
      listener?.({ run_id: runId ?? lastRunId, event });
    },
    getLastRunId: () => lastRunId,
  };
}

function tokenizeLineWithParser(lines: string[]): Array<{ text: string; token: string | null }> {
  const state = algParser.startState!(2);
  const tokens: Array<{ text: string; token: string | null }> = [];
  for (const line of lines) {
    if (line.length === 0) {
      algParser.blankLine?.(state, 2);
      continue;
    }
    const stream = new StringStream(line, 4, 2);
    while (!stream.eol()) {
      stream.start = stream.pos;
      const tok = algParser.token(stream, state);
      const slice = line.slice(stream.start, stream.pos);
      if (slice.trim().length > 0) {
        tokens.push({ text: slice, token: tok });
      }
    }
  }
  return tokens;
}

describe("AlgoritmosLab IDE", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    setRecoverySource(null);
  });

  it("normaliza palavras-chave acentuadas do Portugol", () => {
    expect(algParser.name).toBe("algoritmoslab");
    expect(normalizeAlgWord("FUNÇÃO")).toBe("funcao");
    expect(normalizeAlgWord("EntÃo")).toBe("entao");
    expect(normalizeAlgWord("Lógico")).toBe("logico");
  });

  it("renderiza esqueleto inicial e executa fluxo completo com leia() e vetor expansível", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    expect(screen.getByText("AlgoritmosLab")).toBeTruthy();
    expect(screen.getByText("semnome.alg")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

    act(() => {
      mock.emit({ type: "output", text: "Digite seu nome: " });
      mock.emit({
        type: "awaiting_input",
        var_name: "nome",
        expected_type: "caractere",
        line: 6,
      });
    });

    const inputField = await screen.findByPlaceholderText(/Digite um valor caractere/i);
    fireEvent.change(inputField, { target: { value: "Ana" } });
    fireEvent.submit(inputField.closest("form")!);

    expect(mock.bridge.sendInput).toHaveBeenCalledWith("Ana");

    act(() => {
      mock.emit({ type: "output", text: "Olá, Ana\n" });
      mock.emit({
        type: "finished",
        variables: [
          {
            name: "nome",
            type_label: "caractere",
            value_text: "Ana",
            scope: "global",
          },
          {
            name: "v",
            type_label: "vetor[1..2] de inteiro",
            value_text: "[10, 20]",
            scope: "global",
            total_items: 2,
            items: [
              { index_label: "[1]", value_text: "10" },
              { index_label: "[2]", value_text: "20" },
            ],
          },
        ],
      });
    });

    expect(await screen.findByText(/Concluído/i)).toBeTruthy();
    expect(screen.getByTestId("console-output").textContent).toContain("Olá, Ana");

    // Expandir vetor `v`
    const expandBtn = screen.getByRole("button", { name: "v" });
    fireEvent.click(expandBtn);
    expect(screen.getByText("[1]")).toBeTruthy();
    expect(screen.getByText("20")).toBeTruthy();
  });

  it("controla depuração passo a passo, ignora run_id obsoleto e exibe erro de compilação", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    fireEvent.click(screen.getByRole("button", { name: /Passo \(F8\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

    const firstRunId = mock.getLastRunId();

    act(() => {
      mock.emit({
        type: "paused",
        line: 8,
        call_depth: 1,
        call_stack: ['algoritmo "semnome"', "fatorial"],
        variables: [
          { name: "k", type_label: "inteiro", value_text: "4", scope: "fatorial" },
        ],
      });
    });

    expect(await screen.findByText(/Pausado na linha 8/i)).toBeTruthy();
    expect(screen.getByText("Pilha de Chamadas")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Pular/i }));
    expect(mock.bridge.debugAction).toHaveBeenCalledWith("step_over");

    fireEvent.click(screen.getByRole("button", { name: /Parar/i }));
    expect(mock.bridge.stopExecution).toHaveBeenCalled();

    // Nova execução com outro runId: eventos atrasados da sessão antiga devem ser ignorados
    act(() => {
      mock.emit({ type: "finished", variables: [] });
    });

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(2));

    act(() => {
      mock.emit({ type: "output", text: "SAIDA_ANTIGA" }, firstRunId);
      mock.emit({
        type: "error",
        kind: "sintatico",
        line: 5,
        column: 3,
        message: "Esperado 'fimse'.",
        variables: [],
      });
    });

    const consoleText = screen.getByTestId("console-output").textContent ?? "";
    expect(consoleText).not.toContain("SAIDA_ANTIGA");
    expect(screen.getByRole("alert").textContent).toContain("Esperado 'fimse'.");
  });

  it("alterna Modo Prova, Tema Escuro, carrega exemplo didático e salva em Windows-1252", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    const examBtn = screen.getByRole("button", { name: /Modo Prova/i });
    fireEvent.click(examBtn);
    expect(screen.getByText(/Modo Prova \(Editor Neutro\)/i)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Ativar tema escuro/i }));
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");

    // Abrir galeria e carregar exemplo
    fireEvent.click(screen.getByRole("button", { name: /Exemplos/i }));
    expect(await screen.findByText(/Ordenação BubbleSort/i)).toBeTruthy();
    const loadButtons = screen.getAllByRole("button", {
      name: /Carregar no Editor/i,
    });
    fireEvent.click(loadButtons[0]);

    await waitFor(() =>
      expect(screen.getByText("ola-mundo.alg")).toBeTruthy()
    );

    // Salvar explicitamente em Windows-1252
    fireEvent.click(screen.getByRole("button", { name: /Win-1252/i }));
    await waitFor(() =>
      expect(mock.bridge.saveAlgFile).toHaveBeenCalledWith(
        "/tmp/salvo.alg",
        expect.stringContaining('algoritmo "OlaMundo"'),
        "windows1252"
      )
    );
  });

  it("suporta atalhos F9/F8/Shift+F8 e abre arquivo preservando encoding Windows-1252", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    // Abrir arquivo .alg
    fireEvent.click(screen.getByRole("button", { name: /Abrir/i }));
    await waitFor(() => expect(screen.getByText("teste.alg")).toBeTruthy());
    expect(screen.getAllByText("Win-1252").length).toBeGreaterThanOrEqual(1);

    // Atalho F8 inicia em StepInto
    fireEvent.keyDown(window, { key: "F8" });
    await waitFor(() =>
      expect(mock.bridge.startExecution).toHaveBeenLastCalledWith(
        expect.objectContaining({ mode: "step_into" })
      )
    );

    act(() => {
      mock.emit({
        type: "paused",
        line: 3,
        call_depth: 0,
        call_stack: ['algoritmo "teste"'],
        variables: [],
      });
    });

    // Shift+F8 durante pausa dispara step_over
    fireEvent.keyDown(window, { key: "F8", shiftKey: true });
    await waitFor(() =>
      expect(mock.bridge.debugAction).toHaveBeenCalledWith("step_over")
    );
  });

  it("aplica cores do mudacor mesmo após limpatela, rastreia timer com registro e controla eco off", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

    act(() => {
      mock.emit({ type: "console_color", target: "foreground", color: "#facc15" });
      mock.emit({ type: "console_color", target: "background", color: "#1d4ed8" });
      mock.emit({ type: "output", text: "ANTES\n" });
      mock.emit({ type: "clear" });
      mock.emit({ type: "output", text: "DEPOIS\n" });
      mock.emit({
        type: "trace_line",
        line: 5,
        variables: [
          {
            name: "aluno",
            type_label: "TAluno",
            value_text: '{nome: "Bia", media: 9.5}',
            scope: "global",
            total_items: 2,
            items: [
              { index_label: ".nome", value_text: "Bia" },
              { index_label: ".media", value_text: "9.5" },
            ],
          },
        ],
      });
    });

    const consoleEl = screen.getByTestId("console-output");
    expect(consoleEl.textContent).not.toContain("ANTES");
    expect(consoleEl.textContent).toContain("DEPOIS");
    expect(consoleEl.style.color).toBe("rgb(250, 204, 21)");
    expect(consoleEl.style.backgroundColor).toBe("rgb(29, 78, 216)");
    expect(screen.getByText(/Executando\.\.\./i)).toBeTruthy();

    // Expansão dos campos do registro `aluno`
    fireEvent.click(screen.getByRole("button", { name: "aluno" }));
    expect(screen.getByText(".nome")).toBeTruthy();
    expect(screen.getByText("Bia")).toBeTruthy();

    // Simular leia() com eco off (nenhum evento input_echo) -> valor digitado não aparece ecoado
    act(() => {
      mock.emit({
        type: "awaiting_input",
        var_name: "senha",
        expected_type: "caractere",
        line: 8,
      });
    });

    const inputField = await screen.findByPlaceholderText(/Digite um valor caractere/i);
    fireEvent.change(inputField, { target: { value: "SEGREDO_SEM_ECO" } });
    fireEvent.submit(inputField.closest("form")!);

    expect(mock.bridge.sendInput).toHaveBeenCalledWith("SEGREDO_SEM_ECO");
    expect(screen.getByTestId("console-output").textContent).not.toContain("SEGREDO_SEM_ECO");

    // Com eco on (evento input_echo emitido pelo core) -> valor aparece ecoado
    act(() => {
      mock.emit({ type: "input_echo", text: "ECO_VISIVEL" });
      mock.emit({ type: "finished", variables: [] });
    });
    expect(screen.getByTestId("console-output").textContent).toContain("ECO_VISIVEL");
  });

  it("solicita pasta de trabalho autorizada para algoritmo não salvo que usa comando arquivo", async () => {
    const mock = createMockBridge();
    vi.mocked(mock.bridge.pickWorkingDir).mockResolvedValueOnce(null).mockResolvedValueOnce("/tmp/trabalho");
    vi.mocked(mock.bridge.openAlgFile).mockResolvedValueOnce({
      path: "/tmp/salvo_dir/prog.alg",
      name: "prog.alg",
      text: 'algoritmo "arq"\narquivo "dados.txt"\ninicio\n   escreval("ok")\nfimalgoritmo\n',
      encoding: "utf8",
    });

    render(<App bridge={mock.bridge} />);

    // Abrir arquivo salvo já deriva workingDir automaticamente (/tmp/salvo_dir)
    fireEvent.click(screen.getByRole("button", { name: /Abrir/i }));
    await waitFor(() => expect(screen.getByText("prog.alg")).toBeTruthy());

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() =>
      expect(mock.bridge.startExecution).toHaveBeenLastCalledWith(
        expect.objectContaining({
          options: expect.objectContaining({ sandbox_dir: "/tmp/salvo_dir" }),
        })
      )
    );

    // Finaliza sessão e troca para código novo não salvo
    act(() => {
      mock.emit({ type: "finished", variables: [] });
    });

    // Clicar em "Novo" limpa `manualWorkingDir`
    fireEvent.click(screen.getByRole("button", { name: /Novo/i }));
    await waitFor(() => expect(screen.getByText("semnome.alg")).toBeTruthy());

    // Selecionar pasta manual: primeiro retorno null não altera, segundo retorna "/tmp/trabalho"
    fireEvent.click(screen.getByRole("button", { name: /Pasta arquivo/i }));
    await waitFor(() => expect(mock.bridge.pickWorkingDir).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole("button", { name: /Pasta arquivo/i }));
    await waitFor(() => expect(screen.getByText(/Pasta: trabalho/i)).toBeTruthy());

    expect(
      usesArquivoCommand('algoritmo "a"\nARQUIVO "dados.txt"\ninicio\nfimalgoritmo')
    ).toBe(true);
    expect(
      usesArquivoCommand('algoritmo "a"\n// arquivo "dados.txt"\ninicio\nescreval("arquivo")\nfimalgoritmo')
    ).toBe(false);
    expect(getParentDir("/home/aluno/algoritmos/aula1.alg")).toBe("/home/aluno/algoritmos");
    expect(getParentDir("C:\\Aulas\\prog.alg")).toBe("C:\\Aulas");
  });

  it("trata casos extremos de usesArquivoCommand, suggestFileName NFD, pathUtils e normalizeLineEndings", () => {
    // String com // antes de arquivo (single-pass evita que //x") coma o código!)
    expect(
      usesArquivoCommand('algoritmo "a"\ninicio\n   escreval("http://x"); arquivo "a.txt"\nfimalgoritmo')
    ).toBe(true);
    // Sintaxe com parênteses arquivo("dados.txt")
    expect(
      usesArquivoCommand('algoritmo "a"\narquivo("dados.txt")\ninicio\nfimalgoritmo')
    ).toBe(true);
    // Ignora conteúdo após fimalgoritmo
    expect(
      usesArquivoCommand('algoritmo "a"\ninicio\n   escreval("oi")\nfimalgoritmo\narquivo "depois.txt"')
    ).toBe(false);
    // "fimalgoritmo" dentro de string não interrompe detecção prematuramente
    expect(
      usesArquivoCommand('algoritmo "a"\ninicio\n   escreval("fimalgoritmo")\n   arquivo "dados.txt"\nfimalgoritmo')
    ).toBe(true);

    // suggestFileName com diacríticos NFD
    expect(suggestFileName('algoritmo "Olá"')).toBe("ola.alg");
    expect(suggestFileName('algoritmo "Cálculo Média!"')).toBe("calculo_media.alg");
    expect(suggestFileName('algoritmo "???"', "padrao.alg")).toBe("padrao.alg");

    // normalizeLineEndings e getFileName
    expect(normalizeLineEndings("a\r\nb\rc\n")).toBe("a\nb\nc\n");
    expect(getFileName("/tmp/trabalho/")).toBe("trabalho");
    expect(getFileName("C:\\Aulas\\prog.alg")).toBe("prog.alg");
  });

  it("normaliza CRLF ao abrir Windows-1252 evitando isDirty preso, preserva manualWorkingDir ao salvar cópia e sincroniza encoding", async () => {
    const mock = createMockBridge();
    vi.mocked(mock.bridge.openAlgFile).mockResolvedValueOnce({
      path: "/tmp/win.alg",
      name: "win.alg",
      text: 'algoritmo "win"\r\ninicio\r\n   escreval("Olá")\r\nfimalgoritmo\r\n',
      encoding: "windows1252",
    });

    render(<App bridge={mock.bridge} />);

    // Em documento novo (filePath === null), mudar encoding padrão nas configurações reflete na barra
    fireEvent.click(screen.getByRole("button", { name: /Configurações/i }));
    const encSelect = await screen.findByLabelText(/Encoding padrão ao salvar/i);
    fireEvent.change(encSelect, { target: { value: "windows1252" } });
    expect(screen.getAllByText("Win-1252").length).toBeGreaterThanOrEqual(1);
    fireEvent.click(screen.getByRole("button", { name: /Fechar configurações/i }));

    // Selecionar pasta manual e salvar cópia: manualWorkingDir NÃO deve ser sobrescrito pelo caminho salvo
    vi.mocked(mock.bridge.pickWorkingDir).mockResolvedValueOnce("/tmp/sandbox_manual");
    fireEvent.click(screen.getByRole("button", { name: /Pasta arquivo/i }));
    await waitFor(() => expect(screen.getByText(/Pasta: sandbox_manual/i)).toBeTruthy());

    fireEvent.click(screen.getByRole("button", { name: /Salvar/i }));
    await waitFor(() => expect(mock.bridge.saveAlgFile).toHaveBeenCalled());
    expect(screen.getByText(/Pasta: sandbox_manual/i)).toBeTruthy();

    // Abrir arquivo com CRLF: não fica sujo (*) inicialmente
    fireEvent.click(screen.getByRole("button", { name: /Abrir/i }));
    await waitFor(() => expect(screen.getByText("win.alg")).toBeTruthy());
    expect(screen.queryByTitle("Alterações não salvas")).toBeNull();

    // Digitar e desfazer (Ctrl+Z) limpa o indicador de sujo (*) mesmo tendo aberto com \r\n
    const cmRoot = document.querySelector(".cm-editor");
    expect(cmRoot).toBeTruthy();
    const view = EditorView.findFromDOM(cmRoot as HTMLElement);
    expect(view).not.toBeNull();
    act(() => {
      view!.dispatch({ changes: { from: 0, insert: "// edit\n" } });
    });
    expect(screen.getByTitle("Alterações não salvas")).toBeTruthy();
    act(() => {
      undo(view!);
    });
    expect(screen.queryByTitle("Alterações não salvas")).toBeNull();
  });

  it("reverte estado otimista e exibe erro quando debugAction, sendInput ou startExecution rejeitam", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    // 1) startExecution rejeita
    vi.mocked(mock.bridge.startExecution).mockRejectedValueOnce(new Error("Falha ao iniciar VM"));
    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    expect(await screen.findByText(/Falha ao iniciar VM/i)).toBeTruthy();
    expect(screen.getByRole("status")).toHaveTextContent("Erro");

    // 2) debugAction rejeita enquanto pausado -> volta para Pausado na linha 4
    fireEvent.click(screen.getByRole("button", { name: /Passo \(F8\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(2));

    act(() => {
      mock.emit({
        type: "paused",
        line: 4,
        call_depth: 0,
        call_stack: ['algoritmo "semnome"'],
        variables: [],
      });
    });
    expect(await screen.findByText(/Pausado na linha 4/i)).toBeTruthy();

    vi.mocked(mock.bridge.debugAction).mockRejectedValueOnce(new Error("IPC debug falhou"));
    fireEvent.click(screen.getByRole("button", { name: /Entrar \(F8\)/i }));

    expect(await screen.findByText(/IPC debug falhou/i)).toBeTruthy();
    expect(screen.getByText(/Pausado na linha 4/i)).toBeTruthy();

    // 3) sendInput rejeita enquanto aguardando entrada -> restaura prompt de input
    act(() => {
      mock.emit({
        type: "awaiting_input",
        var_name: "idade",
        expected_type: "inteiro",
        line: 5,
      });
    });

    const inputField = await screen.findByPlaceholderText(/Digite um valor inteiro/i);
    vi.mocked(mock.bridge.sendInput).mockRejectedValueOnce(new Error("IPC input falhou"));
    fireEvent.change(inputField, { target: { value: "25" } });
    fireEvent.submit(inputField.closest("form")!);

    expect(await screen.findByText(/IPC input falhou/i)).toBeTruthy();
    expect(await screen.findByPlaceholderText(/Digite um valor inteiro/i)).toBeTruthy();
  });

  it("evita duplo disparo ao pressionar F8/F9 rapidamente e aborta se documento mudar durante pickWorkingDir", async () => {
    const mock = createMockBridge();
    let resolvePicker: (val: string | null) => void = () => {};
    vi.mocked(mock.bridge.pickWorkingDir).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolvePicker = resolve;
        })
    );
    vi.mocked(mock.bridge.openAlgFile).mockResolvedValueOnce({
      path: "/tmp/arq.alg",
      name: "arq.alg",
      text: 'algoritmo "arq"\narquivo "dados.txt"\ninicio\nfimalgoritmo\n',
      encoding: "utf8",
    });

    render(<App bridge={mock.bridge} />);

    // Carregar conteúdo que usa `arquivo` mas sem filePath (via exemplo? Não, via novo + abrir? Vamos simular editando após abrir, ou mockando filePath null via editor)
    // Melhor: abrir o arquivo e depois usar Novo não serve; podemos testar duplo-tap no F9 direto primeiro!
    fireEvent.keyDown(window, { key: "F9" });
    fireEvent.keyDown(window, { key: "F9" });
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

    act(() => {
      mock.emit({ type: "finished", variables: [] });
    });

    // Agora testar proteção de corrida com pickWorkingDir pendente:
    // Colocar código com `arquivo` em documento não salvo via CodeMirror
    const cmRoot = document.querySelector(".cm-editor") as HTMLElement;
    const cmView = EditorView.findFromDOM(cmRoot)!;
    act(() => {
      cmView.dispatch({
        changes: {
          from: 0,
          to: cmView.state.doc.length,
          insert: 'algoritmo "arq"\narquivo "dados.txt"\ninicio\nfimalgoritmo\n',
        },
      });
    });

    // Clica Executar -> abre pickWorkingDir pendente -> altera documento -> resolve picker -> aborta sem chamar startExecution
    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.pickWorkingDir).toHaveBeenCalledTimes(1));

    act(() => {
      cmView.dispatch({
        changes: { from: 0, insert: "// modificado durante picker\n" },
      });
    });
    await act(async () => {
      resolvePicker("/tmp/trabalho");
    });

    expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1);
    expect(
      await screen.findByText(/algoritmo foi modificado durante a seleção/i)
    ).toBeTruthy();

    // Testar duplo-tap no F8 enquanto paused
    fireEvent.keyDown(window, { key: "F8" });
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(2));

    act(() => {
      mock.emit({
        type: "paused",
        line: 3,
        call_depth: 0,
        call_stack: ['algoritmo "semnome"'],
        variables: [],
      });
    });

    let resolveDebug: () => void = () => {};
    vi.mocked(mock.bridge.debugAction).mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveDebug = resolve;
        })
    );

    fireEvent.keyDown(window, { key: "F8" });
    fireEvent.keyDown(window, { key: "F8" });
    expect(mock.bridge.debugAction).toHaveBeenCalledTimes(1);
    act(() => resolveDebug());
  });

  it("aplica focus trap no modal, restaura foco ao fechar e bloqueia atalhos globais com modal, AltGr ou inputs", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    const settingsBtn = screen.getByRole("button", { name: /Configurações/i });
    settingsBtn.focus();
    fireEvent.click(settingsBtn);

    const dialog = await screen.findByRole("dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);

    // Tab / Shift+Tab ficam presos dentro do modal
    const closeBtn = screen.getByRole("button", { name: /Fechar configurações/i });
    const encodingSelect = screen.getByLabelText(/Encoding padrão ao salvar/i);
    encodingSelect.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(closeBtn);

    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(encodingSelect);

    // Atalhos globais bloqueados enquanto modal está aberto
    fireEvent.keyDown(window, { key: "F9" });
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    expect(mock.bridge.startExecution).not.toHaveBeenCalled();
    expect(mock.bridge.pickSaveAlgPath).not.toHaveBeenCalled();

    // Escape fecha o modal e restaura foco ao botão original
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(document.activeElement).toBe(settingsBtn);

    // AltGr (ctrlKey + altKey) NÃO dispara atalho Ctrl+S
    fireEvent.keyDown(window, { key: "s", ctrlKey: true, altKey: true });
    expect(mock.bridge.pickSaveAlgPath).not.toHaveBeenCalled();

    // Ctrl+N / Ctrl+O / Ctrl+S dentro de #console-leia-input não disparam atalhos globais
    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

    act(() => {
      mock.emit({
        type: "awaiting_input",
        var_name: "x",
        expected_type: "inteiro",
        line: 3,
      });
    });

    const inputField = await screen.findByPlaceholderText(/Digite um valor inteiro/i);
    fireEvent.keyDown(inputField, { key: "n", ctrlKey: true });
    fireEvent.keyDown(inputField, { key: "s", ctrlKey: true });
    expect(mock.bridge.pickSaveAlgPath).not.toHaveBeenCalled();
    expect(mock.bridge.stopExecution).not.toHaveBeenCalled();
  });

  it("impõe ring buffer de console e exibe aviso de truncamento", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

    act(() => {
      for (let i = 0; i < MAX_CONSOLE_CHUNKS + 25; i++) {
        mock.emit({ type: "input_echo", text: `linha_${i}` });
      }
    });

    expect(
      screen.getByText(/Saída anterior truncada para manter o console responsivo/i)
    ).toBeTruthy();
    const stream = screen.getByTestId("console-output");
    expect(stream.textContent).not.toContain("linha_0\n");
    expect(stream.textContent).toContain(`linha_${MAX_CONSOLE_CHUNKS + 24}`);
    expect(stream.textContent!.length).toBeLessThanOrEqual(MAX_CONSOLE_CHARS + 100);
  });

  it("valida e sanitiza configurações inválidas em loadSettings()", () => {
    window.localStorage.setItem(SETTINGS_STORAGE_KEY, "{json invalido");
    expect(loadSettings()).toEqual({
      theme: "light",
      examMode: false,
      strictRead: false,
      defaultSaveEncoding: "utf8",
      fontSize: 15,
    });

    window.localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(["array"]));
    expect(loadSettings().theme).toBe("light");

    window.localStorage.setItem(
      SETTINGS_STORAGE_KEY,
      JSON.stringify({
        theme: "neon",
        examMode: "false",
        strictRead: 1,
        defaultSaveEncoding: "latin1",
        fontSize: 99.6,
      })
    );
    expect(loadSettings()).toEqual({
      theme: "light",
      examMode: false,
      strictRead: false,
      defaultSaveEncoding: "utf8",
      fontSize: 22,
    });

    window.localStorage.setItem(
      SETTINGS_STORAGE_KEY,
      JSON.stringify({
        theme: "dark",
        examMode: true,
        strictRead: true,
        defaultSaveEncoding: "windows1252",
        fontSize: 16.4,
      })
    );
    expect(loadSettings()).toEqual({
      theme: "dark",
      examMode: true,
      strictRead: true,
      defaultSaveEncoding: "windows1252",
      fontSize: 16,
    });
  });

  it("garante contraste seguro do mudacor para apenas FRENTE, cores inválidas e pares ilegíveis", () => {
    expect(isValidHexColor("#fff")).toBe(true);
    expect(isValidHexColor("#1d4ed8")).toBe(true);
    expect(isValidHexColor("red")).toBe(false);
    expect(isValidHexColor("#12345")).toBe(false);

    expect(isDarkHex("#0f172a")).toBe(true);
    expect(isDarkHex("#f8fafc")).toBe(false);
    expect(getContrastRatio("#0f172a", "#f8fafc")).toBeGreaterThan(10);

    // Amarelo apenas em FRENTE no tema claro -> recebe fundo escuro seguro (>= 4.5:1)
    const yellowOnLight = resolveConsoleColors({
      fg: "#facc15",
      bg: null,
      theme: "light",
    });
    expect(yellowOnLight.fg).toBe("#facc15");
    expect(yellowOnLight.bg).not.toBeNull();
    expect(getContrastRatio(yellowOnLight.fg!, yellowOnLight.bg!)).toBeGreaterThanOrEqual(4.5);

    // Preto apenas em FRENTE no tema escuro -> recebe fundo claro seguro (>= 4.5:1)
    const blackOnDark = resolveConsoleColors({
      fg: "#0f172a",
      bg: null,
      theme: "dark",
    });
    expect(blackOnDark.fg).toBe("#0f172a");
    expect(blackOnDark.bg).not.toBeNull();
    expect(getContrastRatio(blackOnDark.fg!, blackOnDark.bg!)).toBeGreaterThanOrEqual(4.5);

    // Preto sobre Preto -> corrige cor do texto para claro
    const blackOnBlack = resolveConsoleColors({
      fg: "#0f172a",
      bg: "#0f172a",
      theme: "light",
    });
    expect(blackOnBlack.bg).toBe("#0f172a");
    expect(getContrastRatio(blackOnBlack.fg!, blackOnBlack.bg!)).toBeGreaterThanOrEqual(4.5);

    // Cor inválida é ignorada
    expect(
      resolveConsoleColors({ fg: "javascript:alert(1)", bg: null, theme: "light" })
    ).toEqual({ fg: null, bg: null });
  });

  it("estiliza matriz e div contextualmente, inclui pot/builtins e fecha bloco com comentário // inline", () => {
    const tokens = tokenizeLineWithParser([
      'algoritmo "ctx"',
      "var",
      "   matriz, div: inteiro",
      "   m: matriz[1..3, 1..3] de inteiro",
      "inicio",
      "   div <- 10 div 3",
      "fimalgoritmo",
    ]);

    const varLineTokens = tokens.filter((t) => t.text === "matriz" || t.text === "div");
    // 1º matriz (var matriz), 1º div (var div), 2º matriz (m: matriz[...]), 2º div (div <-), 3º div (10 div 3)
    expect(varLineTokens[0].token).toBe("variableName");
    expect(varLineTokens[1].token).toBe("variableName");
    expect(varLineTokens[2].token).toBe("typeName");
    expect(varLineTokens[3].token).toBe("variableName");
    expect(varLineTokens[4].token).toBe("keyword");

    // Completions incluem pot, caracter, literal, cotan, etc. e matriz usa vetor[...]
    const labels = new Set(ALG_COMPLETIONS.map((c) => c.label));
    for (const expected of [
      "pot",
      "caracter",
      "literal",
      "cotan",
      "arcsen",
      "arccos",
      "arctan",
      "grauprad",
      "radpgrau",
      "log",
      "logn",
      "asc",
      "carac",
    ]) {
      expect(labels.has(expected)).toBe(true);
    }
    const matrizComp = ALG_COMPLETIONS.find((c) => c.label === "matriz");
    expect(String(matrizComp?.apply)).toContain("vetor[");

    // Auto-close com comentário // inline
    const lineText = "   se x > 0 entao // ok";
    const doc = `algoritmo "t"\ninicio\n${lineText}\nfimalgoritmo`;
    const endOfLinePos = doc.indexOf(lineText) + lineText.length;
    const view = new EditorView({
      state: EditorState.create({
        doc,
        selection: { anchor: endOfLinePos },
      }),
    });
    const handled = tryAutoCloseAlgBlock(view);
    expect(handled).toBe(true);
    expect(view.state.doc.toString()).toContain("   fimse");
    view.destroy();
  });

  it("ErrorBoundary captura falhas de renderização, preserva código não salvo e permite copiar/recarregar", async () => {
    const consoleErrSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const writeTextMock = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: writeTextMock },
      configurable: true,
    });

    setRecoverySource('algoritmo "recuperado"\ninicio\nfimalgoritmo');
    const onReload = vi.fn();

    function Boom(): never {
      throw new Error("Falha simulada no editor");
    }

    render(
      <ErrorBoundary onReload={onReload}>
        <Boom />
      </ErrorBoundary>
    );

    expect(screen.getByRole("alert")).toBeTruthy();
    expect(screen.getByText(/Falha simulada no editor/i)).toBeTruthy();
    const preview = screen.getByLabelText(/Código não salvo preservado/i) as HTMLTextAreaElement;
    expect(preview.value).toContain('algoritmo "recuperado"');

    fireEvent.click(screen.getByRole("button", { name: /Copiar código/i }));
    await waitFor(() =>
      expect(writeTextMock).toHaveBeenCalledWith(
        'algoritmo "recuperado"\ninicio\nfimalgoritmo'
      )
    );
    expect(await screen.findByText(/Código copiado!/i)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Recarregar IDE/i }));
    expect(onReload).toHaveBeenCalledTimes(1);

    consoleErrSpy.mockRestore();
  });

  it("bloqueia edição no CodeEditor durante sessão ativa, suporta redimensionador acessível e guarda não-Tauri na bridge", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    // Resizer acessível por teclado (ArrowUp / ArrowDown)
    const separator = screen.getByRole("separator", {
      name: /Redimensionar altura do console/i,
    });
    expect(separator.getAttribute("aria-valuenow")).toBe("210");
    fireEvent.keyDown(separator, { key: "ArrowUp" });
    expect(separator.getAttribute("aria-valuenow")).toBe("226");
    fireEvent.keyDown(separator, { key: "ArrowDown" });
    expect(separator.getAttribute("aria-valuenow")).toBe("210");

    // Iniciar execução bloqueia editor e exibe aviso somente leitura
    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));
    expect(
      screen.getByText(/Editor bloqueado durante a execução/i)
    ).toBeTruthy();

    // Durante awaiting_input, Pausar fica oculto e aparece badge Aguardando leia()
    act(() => {
      mock.emit({
        type: "awaiting_input",
        var_name: "n",
        expected_type: "inteiro",
        line: 3,
      });
    });
    expect(screen.queryByRole("button", { name: /Pausar/i })).toBeNull();
    expect(screen.getByText(/Aguardando leia\(\)/i)).toBeTruthy();

    // Guardas não-Tauri em tauriIdeBridge
    expect(tauriIdeBridge.isDesktop()).toBe(false);
    await expect(tauriIdeBridge.pickOpenAlgPath()).resolves.toBeNull();
    await expect(tauriIdeBridge.pickSaveAlgPath("x.alg")).resolves.toBeNull();
    await expect(tauriIdeBridge.pickWorkingDir()).resolves.toBeNull();
    await expect(tauriIdeBridge.openAlgFile("/tmp/x.alg")).rejects.toThrow(
      /desktop Tauri/i
    );
    await expect(
      tauriIdeBridge.saveAlgFile("/tmp/x.alg", "algoritmo", "utf8")
    ).rejects.toThrow(/desktop Tauri/i);
  });

  it("protege fechamento da janela via onWindowCloseRequested com bloqueio, stopExecution e guarda contra reentrância", async () => {
    let closeHandler: (() => Promise<boolean>) | null = null;
    const mock = createMockBridge();
    vi.mocked(mock.bridge.onWindowCloseRequested).mockImplementationOnce(
      async (handler) => {
        closeHandler = handler;
        return () => {
          closeHandler = null;
        };
      }
    );

    render(<App bridge={mock.bridge} />);
    await waitFor(() => expect(closeHandler).not.toBeNull());

    if (!closeHandler) {
      throw new Error("closeHandler não registrado");
    }
    const invokeClose: () => Promise<boolean> = closeHandler;

    // Documento limpo -> fecha diretamente chamando stopExecution sem confirmAction
    await expect(invokeClose()).resolves.toBe(true);
    expect(mock.bridge.confirmAction).not.toHaveBeenCalled();
    expect(mock.bridge.stopExecution).toHaveBeenCalledTimes(1);

    // Tornar documento sujo
    const cmRoot = document.querySelector(".cm-editor");
    if (!(cmRoot instanceof HTMLElement)) {
      throw new Error("Editor DOM não encontrado");
    }
    const view = EditorView.findFromDOM(cmRoot);
    if (!view) {
      throw new Error("EditorView não encontrado");
    }
    act(() => {
      view.dispatch({ changes: { from: 0, insert: "// alterado\n" } });
    });

    // Usuário recusa descarte -> retorna false e NÃO chama stopExecution
    vi.mocked(mock.bridge.confirmAction).mockResolvedValueOnce(false);
    await expect(invokeClose()).resolves.toBe(false);
    expect(mock.bridge.stopExecution).toHaveBeenCalledTimes(1);

    // Guarda contra reentrância (closingRef) enquanto confirmAction está pendente
    let resolveConfirm: (val: boolean) => void = () => {};
    vi.mocked(mock.bridge.confirmAction).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveConfirm = resolve;
        })
    );

    const firstAttempt = invokeClose();
    const reentrantAttempt = invokeClose();
    await expect(reentrantAttempt).resolves.toBe(false);
    expect(mock.bridge.confirmAction).toHaveBeenCalledTimes(2);

    resolveConfirm(true);
    await expect(firstAttempt).resolves.toBe(true);
    expect(mock.bridge.stopExecution).toHaveBeenCalledTimes(2);
  });

  it("coalesce múltiplos eventos output via requestAnimationFrame e preserva pendingTruncatedRef", async () => {
    const rafQueue: FrameRequestCallback[] = [];
    const rafSpy = vi
      .spyOn(globalThis, "requestAnimationFrame")
      .mockImplementation((cb: FrameRequestCallback) => {
        rafQueue.push(cb);
        return rafQueue.length;
      });
    const cancelSpy = vi
      .spyOn(globalThis, "cancelAnimationFrame")
      .mockImplementation(() => {});

    try {
      const mock = createMockBridge();
      render(<App bridge={mock.bridge} />);

      fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
      await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

      // Ignora RAFs internos do CodeMirror antes da emissão de saída
      rafSpy.mockClear();
      rafQueue.length = 0;

      // Múltiplos outputs no mesmo frame agendam apenas 1 RAF
      act(() => {
        mock.emit({ type: "output", text: "Parte 1 " });
        mock.emit({ type: "output", text: "Parte 2\n" });
      });
      expect(rafSpy).toHaveBeenCalledTimes(1);
      expect(screen.getByTestId("console-output").textContent).not.toContain("Parte 1");

      act(() => {
        for (const cb of rafQueue.splice(0)) {
          cb(16);
        }
      });
      expect(screen.getByTestId("console-output").textContent).toContain("Parte 1 Parte 2");

      // Agora exceder MAX_CONSOLE_CHARS * 2 dentro de um mesmo frame -> pendingTruncatedRef = true
      const hugeChunk = "Z".repeat(MAX_CONSOLE_CHARS * 2 + 100);
      act(() => {
        mock.emit({ type: "output", text: "DESCARTE_" });
        mock.emit({ type: "output", text: hugeChunk });
      });
      expect(rafSpy).toHaveBeenCalledTimes(2);

      act(() => {
        for (const cb of rafQueue.splice(0)) {
          cb(32);
        }
      });

      expect(
        screen.getByText(/Saída anterior truncada para manter o console responsivo/i)
      ).toBeTruthy();
      const outputText = screen.getByTestId("console-output").textContent ?? "";
      expect(outputText).not.toContain("DESCARTE_");
      expect(outputText.length).toBeLessThanOrEqual(MAX_CONSOLE_CHARS);
    } finally {
      rafSpy.mockRestore();
      cancelSpy.mockRestore();
    }
  });

  it("preserva rascunho no ErrorBoundary ao clicar em Tentar restaurar, permite restaurar/descartar e mantém atalhos Ctrl+S/O/N", async () => {
    const consoleErrSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mock = createMockBridge();
    vi.mocked(mock.bridge.pickSaveAlgPath).mockResolvedValue(null);
    vi.mocked(mock.bridge.pickOpenAlgPath).mockResolvedValue(null);

    let triggerCrash: (() => void) | null = null;
    let shouldThrow = false;

    function CrashableHost() {
      const [crashed, setCrashed] = useState(false);
      triggerCrash = () => {
        shouldThrow = true;
        setCrashed(true);
      };
      if (crashed && shouldThrow) {
        throw new Error("Erro simulado na árvore React");
      }
      return <App bridge={mock.bridge} />;
    }

    render(
      <ErrorBoundary>
        <CrashableHost />
      </ErrorBoundary>
    );

    const cmRoot = document.querySelector(".cm-editor");
    if (!(cmRoot instanceof HTMLElement)) {
      throw new Error("Editor DOM não encontrado");
    }
    const view = EditorView.findFromDOM(cmRoot);
    if (!view) {
      throw new Error("EditorView não encontrado");
    }

    act(() => {
      view.dispatch({
        changes: { from: 0, insert: "// rascunho recuperavel\n" },
      });
    });
    expect(window.localStorage.getItem(RECOVERY_STORAGE_KEY)).toContain(
      "// rascunho recuperavel"
    );

    // Simula falha capturada pelo ErrorBoundary
    act(() => {
      triggerCrash?.();
    });
    expect(screen.getByRole("alert")).toBeTruthy();
    const preview = screen.getByLabelText(/Código não salvo preservado/i);
    if (!(preview instanceof HTMLTextAreaElement)) {
      throw new Error("Textarea de recuperação não encontrado");
    }
    expect(preview.value).toContain("// rascunho recuperavel");

    // Desarma falha antes de clicar em "Tentar restaurar"
    shouldThrow = false;
    fireEvent.click(screen.getByRole("button", { name: /Tentar restaurar/i }));

    // Tentar restaurar NÃO apaga algoritmoslab.recovery.v1 e exibe banner
    expect(window.localStorage.getItem(RECOVERY_STORAGE_KEY)).toContain(
      "// rascunho recuperavel"
    );
    expect(getRecoverySource()).toContain("// rascunho recuperavel");
    expect(
      await screen.findByText(/Rascunho não salvo encontrado após interrupção/i)
    ).toBeTruthy();

    // Atalhos Ctrl+S / Ctrl+O / Ctrl+N continuam funcionando com banner visível
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    await waitFor(() => expect(mock.bridge.pickSaveAlgPath).toHaveBeenCalledTimes(1));
    fireEvent.keyDown(window, { key: "o", ctrlKey: true });
    await waitFor(() => expect(mock.bridge.pickOpenAlgPath).toHaveBeenCalledTimes(1));
    fireEvent.keyDown(window, { key: "n", ctrlKey: true });
    await waitFor(() => expect(screen.getByText("semnome.alg")).toBeTruthy());

    // Novo mount com rascunho salvo -> clicar em "Restaurar no editor" aplica o código
    setRecoverySource('algoritmo "restaurado"\ninicio\n   escreval(42)\nfimalgoritmo');
    const secondRender = render(<App bridge={mock.bridge} />);
    fireEvent.click(
      screen.getByRole("button", { name: /Restaurar no editor/i })
    );
    await waitFor(() =>
      expect(
        screen.queryByText(/Rascunho não salvo encontrado após interrupção/i)
      ).toBeNull()
    );
    const editors = document.querySelectorAll(".cm-editor");
    const lastEditor = editors[editors.length - 1];
    if (!(lastEditor instanceof HTMLElement)) {
      throw new Error("Editor restaurado não encontrado");
    }
    const restoredView = EditorView.findFromDOM(lastEditor);
    if (!restoredView) {
      throw new Error("EditorView restaurado não encontrado");
    }
    expect(restoredView.state.doc.toString()).toContain('algoritmo "restaurado"');
    expect(window.localStorage.getItem(RECOVERY_STORAGE_KEY)).toContain(
      'algoritmo "restaurado"'
    );
    secondRender.unmount();

    // Descartar rascunho limpa o banner e remove algoritmoslab.recovery.v1
    setRecoverySource('algoritmo "descartar"\ninicio\nfimalgoritmo');
    const thirdRender = render(<App bridge={mock.bridge} />);
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /Descartar rascunho/i })
      ).toBeTruthy()
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Descartar rascunho/i })
    );
    expect(
      screen.queryByText(/Rascunho não salvo encontrado após interrupção/i)
    ).toBeNull();
    expect(window.localStorage.getItem(RECOVERY_STORAGE_KEY)).toBeNull();
    await act(async () => {
      await Promise.resolve();
    });
    thirdRender.unmount();

    consoleErrSpy.mockRestore();
  });

  it("impede que performSave pendente sobrescreva documento trocado por handleNew ou loadExample", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    // Caso 1: handleNew ocorre enquanto pickSaveAlgPath está pendente
    let resolveSavePick: (path: string | null) => void = () => {};
    vi.mocked(mock.bridge.pickSaveAlgPath).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveSavePick = resolve;
        })
    );

    const cmRoot = document.querySelector(".cm-editor");
    if (!(cmRoot instanceof HTMLElement)) {
      throw new Error("Editor DOM não encontrado");
    }
    const view = EditorView.findFromDOM(cmRoot);
    if (!view) {
      throw new Error("EditorView não encontrado");
    }
    act(() => {
      view.dispatch({ changes: { from: 0, insert: "// doc 1\n" } });
    });

    fireEvent.click(screen.getByRole("button", { name: /Salvar/i }));
    await waitFor(() => expect(mock.bridge.pickSaveAlgPath).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole("button", { name: /Novo/i }));
    await waitFor(() => expect(screen.queryByTitle("Alterações não salvas")).toBeNull());

    await act(async () => {
      resolveSavePick("/tmp/atrasado_pick.alg");
    });
    expect(mock.bridge.saveAlgFile).not.toHaveBeenCalled();
    expect(screen.getByText("semnome.alg")).toBeTruthy();

    // Caso 2: loadExample ocorre enquanto saveAlgFile está pendente
    let resolveSaveFile: () => void = () => {};
    vi.mocked(mock.bridge.pickSaveAlgPath).mockResolvedValueOnce("/tmp/antigo.alg");
    vi.mocked(mock.bridge.saveAlgFile).mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveSaveFile = resolve;
        })
    );

    act(() => {
      view.dispatch({ changes: { from: 0, insert: "// doc 2\n" } });
    });
    fireEvent.click(screen.getByRole("button", { name: /Salvar/i }));
    await waitFor(() => expect(mock.bridge.saveAlgFile).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole("button", { name: /Exemplos/i }));
    const loadButtons = await screen.findAllByRole("button", {
      name: /Carregar no Editor/i,
    });
    fireEvent.click(loadButtons[0]);
    await waitFor(() => expect(screen.getByText("ola-mundo.alg")).toBeTruthy());

    await act(async () => {
      resolveSaveFile();
    });

    expect(screen.getByText("ola-mundo.alg")).toBeTruthy();
    expect(screen.queryByText("antigo.alg")).toBeNull();
    expect(screen.queryByTitle("Alterações não salvas")).toBeNull();
  });

  it("suporta caminhos Windows UNC/verbatim em pathUtils e destaca matriz em caso ...: como variableName", () => {
    expect(getParentDir("\\\\?\\C:\\algo.alg")).toBe("\\\\?\\C:\\");
    expect(getParentDir("\\\\?\\C:\\pasta\\algo.alg")).toBe("\\\\?\\C:\\pasta");
    expect(getParentDir("\\\\?\\C:\\")).toBeNull();
    expect(getParentDir("\\\\?\\C:")).toBeNull();
    expect(getParentDir("\\\\servidor\\pasta\\arq.alg")).toBe("\\\\servidor\\pasta");
    expect(getParentDir("\\\\servidor\\pasta")).toBeNull();
    expect(getParentDir("\\\\?\\UNC\\servidor\\pasta\\arq.alg")).toBe(
      "\\\\?\\UNC\\servidor\\pasta"
    );

    expect(getFileName("\\\\?\\C:\\algo.alg")).toBe("algo.alg");
    expect(getFileName("\\\\?\\C:\\")).toBe("\\\\?\\C:\\");
    expect(getFileName("\\\\servidor\\pasta\\arq.alg")).toBe("arq.alg");

    const tokens = tokenizeLineWithParser([
      'algoritmo "escolha_matriz"',
      "var",
      "   matriz: vetor[1..3] de inteiro",
      "   op: inteiro",
      "inicio",
      "   escolha op",
      "      caso 1: matriz[1] <- 10",
      "   fimescolha",
      "fimalgoritmo",
    ]);
    const matrizTokens = tokens.filter((t) => t.text === "matriz");
    expect(matrizTokens).toHaveLength(2);
    expect(matrizTokens[0].token).toBe("variableName");
    expect(matrizTokens[1].token).toBe("variableName");

    const standaloneTokens = tokenizeLineWithParser([
      "   caso 1: matriz[1] <- 10",
    ]);
    const standaloneMatriz = standaloneTokens.find((t) => t.text === "matriz");
    expect(standaloneMatriz?.token).toBe("variableName");
  });

  it("exibe rótulos e metadados padronizados para AlgoritmosLab e Portugol", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    expect(
      screen.getByTitle("AlgoritmosLab — IDE de Portugol Estruturado")
    ).toBeTruthy();
    expect(
      screen.getByTitle(
        "Salvar cópia em Windows-1252 (compatível com ambientes legados em Windows)"
      )
    ).toBeTruthy();
    expect(screen.getByLabelText("Editor de código Portugol")).toBeTruthy();
    expect(await screen.findByText("algoritmoslab-core v0.2.0")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Exemplos/i }));
    expect(
      await screen.findByText("Exemplos Didáticos (.alg)")
    ).toBeTruthy();
    expect(
      screen.getByText(
        /Estrutura mínima de um algoritmo em Portugol estruturado/i
      )
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Fechar exemplos/i }));

    fireEvent.click(screen.getByRole("button", { name: /Configurações/i }));
    expect(
      await screen.findByText(/direta com ambientes legados em Windows\./i)
    ).toBeTruthy();
  });

  it("limpa rascunho de recuperação ao confirmar fechamento da janela com documento sujo", async () => {
    let closeHandler: (() => Promise<boolean>) | null = null;
    const mock = createMockBridge();
    vi.mocked(mock.bridge.onWindowCloseRequested).mockImplementationOnce(
      async (handler) => {
        closeHandler = handler;
        return () => {
          closeHandler = null;
        };
      }
    );

    render(<App bridge={mock.bridge} />);
    await waitFor(() => expect(closeHandler).not.toBeNull());
    if (!closeHandler) {
      throw new Error("closeHandler não registrado");
    }
    const invokeClose: () => Promise<boolean> = closeHandler;

    const cmRoot = document.querySelector(".cm-editor");
    if (!(cmRoot instanceof HTMLElement)) {
      throw new Error("Editor DOM não encontrado");
    }
    const view = EditorView.findFromDOM(cmRoot);
    if (!view) {
      throw new Error("EditorView não encontrado");
    }

    act(() => {
      view.dispatch({ changes: { from: 0, insert: "// descarte intencional\n" } });
    });
    expect(window.localStorage.getItem(RECOVERY_STORAGE_KEY)).toContain(
      "// descarte intencional"
    );

    // Recusar fechamento mantém o rascunho
    vi.mocked(mock.bridge.confirmAction).mockResolvedValueOnce(false);
    await expect(invokeClose()).resolves.toBe(false);
    expect(window.localStorage.getItem(RECOVERY_STORAGE_KEY)).toContain(
      "// descarte intencional"
    );

    // Confirmar fechamento limpa o rascunho do localStorage
    vi.mocked(mock.bridge.confirmAction).mockResolvedValueOnce(true);
    await act(async () => {
      await expect(invokeClose()).resolves.toBe(true);
    });
    expect(window.localStorage.getItem(RECOVERY_STORAGE_KEY)).toBeNull();
    expect(getRecoverySource()).toBeNull();
  });

  it("renderiza resumo não interativo quando diagnostic.line é null e botão diagnostic-jump quando line !== null", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    // Erro sem linha (line: null) -> div.diagnostic-summary, sem botão .diagnostic-jump
    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(1));

    act(() => {
      mock.emit({
        type: "error",
        kind: "execucao",
        line: null,
        column: null,
        message: "Falha geral sem linha.",
        variables: [],
      });
    });

    const alertWithoutLine = screen.getByRole("alert");
    expect(alertWithoutLine.textContent).toContain("Falha geral sem linha.");
    expect(alertWithoutLine.querySelector(".diagnostic-jump")).toBeNull();
    const summaryEl = alertWithoutLine.querySelector(".diagnostic-summary");
    expect(summaryEl).not.toBeNull();
    expect(summaryEl?.tagName).toBe("DIV");

    // Erro com linha (line: 4) -> button.diagnostic-jump interativo
    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() => expect(mock.bridge.startExecution).toHaveBeenCalledTimes(2));

    act(() => {
      mock.emit({
        type: "error",
        kind: "sintatico",
        line: 4,
        column: 2,
        message: "Erro na linha 4.",
        variables: [],
      });
    });

    const alertWithLine = screen.getByRole("alert");
    expect(alertWithLine.querySelector(".diagnostic-summary")).toBeNull();
    const jumpBtn = alertWithLine.querySelector("button.diagnostic-jump");
    expect(jumpBtn).not.toBeNull();
    expect(alertWithLine.textContent).toContain("(linha 4:2)");

    if (!(jumpBtn instanceof HTMLButtonElement)) {
      throw new Error("Botão .diagnostic-jump não encontrado");
    }
    fireEvent.click(jumpBtn);

    const cmRoot = document.querySelector(".cm-editor");
    if (!(cmRoot instanceof HTMLElement)) {
      throw new Error("Editor DOM não encontrado");
    }
    const view = EditorView.findFromDOM(cmRoot);
    if (!view) {
      throw new Error("EditorView não encontrado");
    }
    const activeLine = view.state.doc.lineAt(view.state.selection.main.head);
    expect(activeLine.number).toBe(4);
  });

  it("persiste fontSize e strictRead nas Configurações e propaga strictRead para startExecution", async () => {
    const mock = createMockBridge();
    render(<App bridge={mock.bridge} />);

    fireEvent.click(screen.getByRole("button", { name: /Configurações/i }));

    const fontSlider = await screen.findByLabelText(/Tamanho da fonte do editor/i);
    fireEvent.change(fontSlider, { target: { value: "19" } });
    expect(screen.getByText("Atual: 19px")).toBeTruthy();

    const strictToggle = screen.getByLabelText(/Leitura estrita no leia\(\)/i);
    if (!(strictToggle instanceof HTMLInputElement)) {
      throw new Error("Checkbox de leitura estrita não encontrado");
    }
    expect(strictToggle.checked).toBe(false);
    fireEvent.click(strictToggle);
    expect(strictToggle.checked).toBe(true);

    const persisted = loadSettings();
    expect(persisted.fontSize).toBe(19);
    expect(persisted.strictRead).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: /Fechar configurações/i }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() =>
      expect(mock.bridge.startExecution).toHaveBeenLastCalledWith(
        expect.objectContaining({
          options: expect.objectContaining({ strict_read: true }),
        })
      )
    );
  });

  it("alterna breakpoint na linha ativa com Ctrl+B e ignora pickWorkingDir obsoleto após trocar documento", async () => {
    const mock = createMockBridge();
    let resolveWorkingDir: (dir: string | null) => void = () => {};
    vi.mocked(mock.bridge.pickWorkingDir).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveWorkingDir = resolve;
        })
    );

    render(<App bridge={mock.bridge} />);

    const cmRoot = document.querySelector(".cm-editor");
    if (!(cmRoot instanceof HTMLElement)) {
      throw new Error("Editor DOM não encontrado");
    }
    const view = EditorView.findFromDOM(cmRoot);
    if (!view) {
      throw new Error("EditorView não encontrado");
    }

    const line3 = view.state.doc.line(3);
    act(() => {
      view.dispatch({ selection: { anchor: line3.from } });
    });

    fireEvent.keyDown(view.contentDOM, { key: "b", ctrlKey: true });
    expect(await screen.findByText("1 breakpoint")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() =>
      expect(mock.bridge.startExecution).toHaveBeenLastCalledWith(
        expect.objectContaining({ breakpoints: [3] })
      )
    );
    act(() => {
      mock.emit({ type: "finished", variables: [] });
    });

    fireEvent.keyDown(view.contentDOM, { key: "b", ctrlKey: true });
    await waitFor(() => expect(screen.queryByText(/breakpoint/i)).toBeNull());

    fireEvent.click(screen.getByRole("button", { name: /Executar \(F9\)/i }));
    await waitFor(() =>
      expect(mock.bridge.startExecution).toHaveBeenLastCalledWith(
        expect.objectContaining({ breakpoints: [] })
      )
    );
    act(() => {
      mock.emit({ type: "finished", variables: [] });
    });

    // Se documento mudar enquanto pickWorkingDir estiver aberto, seleção atrasada é descartada
    fireEvent.click(screen.getByRole("button", { name: /Pasta arquivo/i }));
    await waitFor(() => expect(mock.bridge.pickWorkingDir).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole("button", { name: /Novo/i }));
    await waitFor(() => expect(screen.getByText("semnome.alg")).toBeTruthy());

    await act(async () => {
      resolveWorkingDir("/tmp/dir_atrasado");
    });
    expect(screen.queryByText(/Pasta: dir_atrasado/i)).toBeNull();
  });
});
