import {
  autocompletion,
  Completion,
  CompletionContext,
  CompletionResult,
  completionStatus,
} from "@codemirror/autocomplete";
import {
  HighlightStyle,
  indentUnit,
  StreamLanguage,
  StreamParser,
  syntaxHighlighting,
} from "@codemirror/language";
import { EditorSelection, Extension } from "@codemirror/state";
import { EditorView, KeyBinding } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import {
  normalizeAlgWord,
  stripCommentsAndMaskStrings,
} from "../utils/algoritmosText";

export { normalizeAlgWord } from "../utils/algoritmosText";

const KEYWORDS = new Set([
  "algoritmo",
  "fimalgoritmo",
  "var",
  "inicio",
  "const",
  "tipo",
  "registro",
  "fimregistro",
  "arquivo",
  "aleatorio",
  "timer",
  "cronometro",
  "pausa",
  "debug",
  "eco",
  "mudacor",
  "dos",
  "de",
  "se",
  "entao",
  "senao",
  "fimse",
  "escolha",
  "caso",
  "outrocaso",
  "fimescolha",
  "para",
  "ate",
  "passo",
  "faca",
  "fimpara",
  "enquanto",
  "fimenquanto",
  "repita",
  "interrompa",
  "procedimento",
  "fimprocedimento",
  "funcao",
  "fimfuncao",
  "retorne",
  "e",
  "ou",
  "xou",
  "nao",
  "mod",
  "escreva",
  "escreval",
  "leia",
  "limpatela",
]);

const TYPES = new Set([
  "inteiro",
  "real",
  "caractere",
  "caracter",
  "literal",
  "logico",
  "vetor",
]);

const BOOLS = new Set(["verdadeiro", "falso"]);

const BUILTINS = new Set([
  "abs",
  "int",
  "quad",
  "raizq",
  "exp",
  "pot",
  "log",
  "logn",
  "pi",
  "sen",
  "cos",
  "tan",
  "cotan",
  "arcsen",
  "arccos",
  "arctan",
  "grauprad",
  "radpgrau",
  "rand",
  "randi",
  "compr",
  "maiusc",
  "minusc",
  "copia",
  "pos",
  "asc",
  "carac",
  "numpcarac",
  "caracpnum",
]);

type BlockKind =
  | "const"
  | "tipo"
  | "var"
  | "registro"
  | "inicio"
  | "se"
  | "para"
  | "enquanto"
  | "repita"
  | "escolha"
  | "caso";

type PrevTokenKind = "sol" | "operand" | "operator" | "punct" | "keyword";

interface AlgStreamState {
  blocks: BlockKind[];
  atLineStart: boolean;
  prevTokenKind: PrevTokenKind;
  expectType: boolean;
  inCaseHeader: boolean;
}

function topBlock(blocks: BlockKind[]): BlockKind | undefined {
  return blocks[blocks.length - 1];
}

function popHeaderSection(blocks: BlockKind[]) {
  const top = topBlock(blocks);
  if (top === "const" || top === "tipo" || top === "var") {
    blocks.pop();
  }
}

function updateBlocksForLineWord(state: AlgStreamState, norm: string) {
  switch (norm) {
    case "algoritmo":
    case "procedimento":
    case "funcao":
      state.blocks = [];
      break;
    case "const":
      popHeaderSection(state.blocks);
      state.blocks.push("const");
      break;
    case "tipo":
      popHeaderSection(state.blocks);
      state.blocks.push("tipo");
      break;
    case "var":
      popHeaderSection(state.blocks);
      state.blocks.push("var");
      break;
    case "registro":
      state.blocks.push("registro");
      break;
    case "fimregistro":
      while (state.blocks.length > 0) {
        const popped = state.blocks.pop();
        if (popped === "registro") break;
      }
      break;
    case "inicio":
      popHeaderSection(state.blocks);
      state.blocks.push("inicio");
      break;
    case "se":
      state.blocks.push("se");
      break;
    case "fimse":
      while (state.blocks.length > 0) {
        const popped = state.blocks.pop();
        if (popped === "se") break;
      }
      break;
    case "para":
      state.blocks.push("para");
      break;
    case "fimpara":
      while (state.blocks.length > 0) {
        const popped = state.blocks.pop();
        if (popped === "para") break;
      }
      break;
    case "enquanto":
      state.blocks.push("enquanto");
      break;
    case "fimenquanto":
      while (state.blocks.length > 0) {
        const popped = state.blocks.pop();
        if (popped === "enquanto") break;
      }
      break;
    case "repita":
      state.blocks.push("repita");
      break;
    case "ate":
      if (topBlock(state.blocks) === "repita") {
        state.blocks.pop();
      }
      break;
    case "escolha":
      state.blocks.push("escolha");
      break;
    case "caso":
    case "outrocaso":
      if (topBlock(state.blocks) === "caso") {
        state.blocks.pop();
      }
      if (topBlock(state.blocks) === "escolha") {
        state.blocks.push("caso");
      }
      break;
    case "fimescolha":
      if (topBlock(state.blocks) === "caso") {
        state.blocks.pop();
      }
      while (state.blocks.length > 0) {
        const popped = state.blocks.pop();
        if (popped === "escolha") break;
      }
      break;
    case "fimprocedimento":
    case "fimfuncao":
    case "fimalgoritmo":
      state.blocks = [];
      break;
  }
}

export const algParser: StreamParser<AlgStreamState> = {
  name: "algoritmoslab",

  startState(): AlgStreamState {
    return {
      blocks: [],
      atLineStart: true,
      prevTokenKind: "sol",
      expectType: false,
      inCaseHeader: false,
    };
  },

  copyState(state: AlgStreamState): AlgStreamState {
    return {
      blocks: [...state.blocks],
      atLineStart: state.atLineStart,
      prevTokenKind: state.prevTokenKind,
      expectType: state.expectType,
      inCaseHeader: state.inCaseHeader,
    };
  },

  token(stream, state): string | null {
    if (stream.sol()) {
      state.atLineStart = true;
      state.prevTokenKind = "sol";
      state.inCaseHeader = false;
    }

    if (stream.eatSpace()) {
      return null;
    }

    // Comentário //
    if (stream.match("//")) {
      stream.skipToEnd();
      state.atLineStart = false;
      return "lineComment";
    }

    // String "..."
    if (stream.peek() === '"') {
      stream.next();
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === '"') break;
      }
      state.atLineStart = false;
      state.prevTokenKind = "operand";
      state.expectType = false;
      return "string";
    }

    // Operadores compostos
    if (
      stream.match("<-") ||
      stream.match("<>") ||
      stream.match("<=") ||
      stream.match(">=")
    ) {
      state.atLineStart = false;
      state.prevTokenKind = "operator";
      state.expectType = false;
      return "operator";
    }

    if (stream.match("..")) {
      state.atLineStart = false;
      state.prevTokenKind = "punct";
      state.expectType = false;
      return "punctuation";
    }

    // Números (cuidando de 1..10 sem consumir '..')
    if (stream.match(/^[0-9]+(\.[0-9]+)?(?![0-9.])/)) {
      state.atLineStart = false;
      state.prevTokenKind = "operand";
      state.expectType = false;
      return "number";
    }
    if (stream.match(/^[0-9]+/)) {
      state.atLineStart = false;
      state.prevTokenKind = "operand";
      state.expectType = false;
      return "number";
    }

    // Operadores simples
    const opCh = stream.eat(/[=<>+\-*/\\%^]/);
    if (opCh) {
      state.atLineStart = false;
      state.prevTokenKind = "operator";
      state.expectType = opCh === "=" && topBlock(state.blocks) === "tipo";
      return "operator";
    }

    // Pontuação
    const punctCh = stream.eat(/[()[\],:;.]/);
    if (punctCh) {
      state.atLineStart = false;
      state.prevTokenKind =
        punctCh === ")" || punctCh === "]" ? "operand" : "punct";
      const top = topBlock(state.blocks);
      const isDeclContext =
        top === undefined || top === "var" || top === "registro";
      state.expectType =
        punctCh === ":" && isDeclContext && !state.inCaseHeader;
      if (punctCh === ":") {
        state.inCaseHeader = false;
      }
      return "punctuation";
    }

    // Identificadores / palavras-chave (suportando letras acentuadas)
    const wordMatch = stream.match(/^[A-Za-zÀ-ÖØ-öø-ÿ_][A-Za-zÀ-ÖØ-öø-ÿ0-9_]*/);
    if (wordMatch) {
      const raw = typeof wordMatch === "boolean" ? stream.current() : wordMatch[0];
      const norm = normalizeAlgWord(raw);
      const wasExpectType = state.expectType;
      const prevKind = state.prevTokenKind;

      if (state.atLineStart || norm === "registro" || norm === "fimregistro") {
        updateBlocksForLineWord(state, norm);
      }
      if (norm === "caso" || norm === "outrocaso") {
        state.inCaseHeader = true;
      }
      state.atLineStart = false;

      if (norm === "matriz") {
        if (wasExpectType && Boolean(stream.match(/^\s*\[/, false))) {
          state.expectType = false;
          state.prevTokenKind = "keyword";
          return "typeName";
        }
        state.expectType = false;
        state.prevTokenKind = "operand";
        return "variableName";
      }

      if (norm === "div") {
        if (
          prevKind === "operand" &&
          !stream.match(/^\s*(?:[:,)\]]|<-)/, false)
        ) {
          state.expectType = false;
          state.prevTokenKind = "operator";
          return "keyword";
        }
        state.expectType = false;
        state.prevTokenKind = "operand";
        return "variableName";
      }

      state.expectType =
        norm === "de" && topBlock(state.blocks) !== "para";

      if (KEYWORDS.has(norm)) {
        state.prevTokenKind = "keyword";
        return "keyword";
      }
      if (TYPES.has(norm)) {
        state.prevTokenKind = "keyword";
        return "typeName";
      }
      if (BOOLS.has(norm)) {
        state.prevTokenKind = "operand";
        return "bool";
      }
      if (BUILTINS.has(norm)) {
        state.prevTokenKind = "operand";
        return "standardName";
      }
      state.prevTokenKind = "operand";
      return "variableName";
    }

    stream.next();
    state.atLineStart = false;
    state.expectType = false;
    state.prevTokenKind = "punct";
    return null;
  },

  blankLine(state) {
    state.atLineStart = true;
    state.prevTokenKind = "sol";
    state.inCaseHeader = false;
  },

  indent(state, textAfter, context): number | null {
    const trimmed = textAfter.trim();
    const firstWordMatch = trimmed.match(/^[A-Za-zÀ-ÖØ-öø-ÿ_]+/);
    const firstNorm = firstWordMatch ? normalizeAlgWord(firstWordMatch[0]) : "";

    let level = state.blocks.length;

    if (
      firstNorm === "const" ||
      firstNorm === "tipo" ||
      firstNorm === "var" ||
      firstNorm === "inicio" ||
      firstNorm === "fimalgoritmo" ||
      firstNorm === "fimprocedimento" ||
      firstNorm === "fimfuncao" ||
      firstNorm === "procedimento" ||
      firstNorm === "funcao"
    ) {
      return 0;
    }

    if (
      firstNorm === "senao" ||
      firstNorm === "fimse" ||
      firstNorm === "fimpara" ||
      firstNorm === "fimenquanto" ||
      firstNorm === "fimregistro"
    ) {
      level = Math.max(0, level - 1);
    } else if (firstNorm === "ate" && topBlock(state.blocks) === "repita") {
      level = Math.max(0, level - 1);
    } else if (firstNorm === "caso" || firstNorm === "outrocaso") {
      if (topBlock(state.blocks) === "caso") {
        level = Math.max(0, level - 1);
      }
    } else if (firstNorm === "fimescolha") {
      if (topBlock(state.blocks) === "caso") {
        level = Math.max(0, level - 2);
      } else {
        level = Math.max(0, level - 1);
      }
    }

    return level * context.unit;
  },

  languageData: {
    commentTokens: { line: "//" },
    closeBrackets: { brackets: ["(", "[", '"'] },
    indentOnInput:
      /^\s*(fimse|senao|fimpara|fimenquanto|ate|fimescolha|fimregistro|fimalgoritmo|fimprocedimento|fimfuncao|inicio|var|tipo|const|caso|outrocaso)\b/i,
  },
};

export const algLanguage = StreamLanguage.define(algParser);

export const algHighlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--syntax-keyword)", fontWeight: "600" },
  { tag: tags.typeName, color: "var(--syntax-type)", fontWeight: "600" },
  { tag: tags.bool, color: "var(--syntax-bool)", fontWeight: "600" },
  { tag: tags.standard(tags.name), color: "var(--syntax-builtin)" },
  { tag: tags.string, color: "var(--syntax-string)" },
  { tag: tags.number, color: "var(--syntax-number)" },
  { tag: tags.lineComment, color: "var(--syntax-comment)", fontStyle: "italic" },
  { tag: tags.operator, color: "var(--syntax-operator)", fontWeight: "500" },
  { tag: tags.punctuation, color: "var(--syntax-punctuation)" },
]);

export const algLanguageExtension: Extension = [
  algLanguage,
  indentUnit.of("   "),
  syntaxHighlighting(algHighlightStyle),
];

export const ALG_COMPLETIONS: Completion[] = [
  { label: "algoritmo", type: "keyword", detail: "início do programa" },
  { label: "const", type: "keyword", detail: "seção de constantes" },
  { label: "tipo", type: "keyword", detail: "seção de tipos compostos" },
  {
    label: "registro",
    type: "keyword",
    detail: "estrutura com campos",
    apply: "registro\n   \nfimregistro",
  },
  { label: "fimregistro", type: "keyword", detail: "fecha registro" },
  { label: "var", type: "keyword", detail: "seção de variáveis" },
  { label: "inicio", type: "keyword", detail: "bloco de comandos" },
  { label: "fimalgoritmo", type: "keyword", detail: "fim do programa" },
  { label: "inteiro", type: "type", detail: "tipo numérico inteiro" },
  { label: "real", type: "type", detail: "tipo ponto flutuante" },
  { label: "caractere", type: "type", detail: "tipo texto" },
  { label: "caracter", type: "type", detail: "sinônimo de caractere" },
  { label: "literal", type: "type", detail: "sinônimo de caractere" },
  { label: "logico", type: "type", detail: "verdadeiro ou falso" },
  { label: "vetor", type: "type", detail: "vetor[ini..fim] de tipo" },
  {
    label: "matriz",
    type: "type",
    detail: "matriz 2D (vetor[l1..l2, c1..c2] de tipo)",
    apply: "vetor[1..3, 1..3] de ",
  },
  { label: "verdadeiro", type: "constant", detail: "literal lógico" },
  { label: "falso", type: "constant", detail: "literal lógico" },
  {
    label: "arquivo",
    type: "keyword",
    detail: "leitura/escrita em arquivo texto",
    apply: 'arquivo "dados.txt"',
  },
  {
    label: "aleatorio",
    type: "keyword",
    detail: "gera entradas automáticas para leia()",
    apply: "aleatorio on",
  },
  { label: "timer", type: "keyword", detail: "atraso entre linhas (ms)" },
  { label: "cronometro", type: "keyword", detail: "mede tempo decorrido" },
  { label: "pausa", type: "keyword", detail: "pausa execução no depurador" },
  { label: "debug", type: "keyword", detail: "pausa condicional" },
  { label: "eco", type: "keyword", detail: "liga/desliga eco do leia()" },
  {
    label: "mudacor",
    type: "keyword",
    detail: "cor do console (FRENTE ou FUNDOS)",
    apply: 'mudacor("Amarelo", "FRENTE")',
  },
  {
    label: "escreval",
    type: "function",
    detail: "escreve e quebra linha",
    apply: 'escreval("")',
  },
  {
    label: "escreva",
    type: "function",
    detail: "escreve sem quebrar linha",
    apply: 'escreva("")',
  },
  {
    label: "leia",
    type: "function",
    detail: "lê valor do console",
    apply: "leia()",
  },
  { label: "limpatela", type: "function", detail: "limpa o console" },
  {
    label: "se",
    type: "keyword",
    detail: "bloco condicional",
    apply: "se  entao\n   \nfimse",
  },
  { label: "senao", type: "keyword", detail: "ramo alternativo" },
  { label: "fimse", type: "keyword", detail: "fecha se" },
  {
    label: "para",
    type: "keyword",
    detail: "laço contado",
    apply: "para i de 1 ate 10 faca\n   \nfimpara",
  },
  { label: "passo", type: "keyword", detail: "incremento do para" },
  { label: "fimpara", type: "keyword", detail: "fecha para" },
  {
    label: "enquanto",
    type: "keyword",
    detail: "laço pré-testado",
    apply: "enquanto  faca\n   \nfimenquanto",
  },
  { label: "fimenquanto", type: "keyword", detail: "fecha enquanto" },
  {
    label: "repita",
    type: "keyword",
    detail: "laço pós-testado",
    apply: "repita\n   \nate ",
  },
  {
    label: "escolha",
    type: "keyword",
    detail: "seleção múltipla",
    apply: "escolha \n   caso \n      \n   outrocaso\n      \nfimescolha",
  },
  { label: "caso", type: "keyword", detail: "ramo do escolha" },
  { label: "outrocaso", type: "keyword", detail: "padrão do escolha" },
  { label: "fimescolha", type: "keyword", detail: "fecha escolha" },
  { label: "interrompa", type: "keyword", detail: "sai do laço" },
  { label: "procedimento", type: "keyword", detail: "declara procedimento" },
  { label: "fimprocedimento", type: "keyword", detail: "fecha procedimento" },
  { label: "funcao", type: "keyword", detail: "declara função" },
  { label: "fimfuncao", type: "keyword", detail: "fecha função" },
  { label: "retorne", type: "keyword", detail: "retorna da sub-rotina" },
  { label: "raizq", type: "function", detail: "raiz quadrada" },
  { label: "abs", type: "function", detail: "valor absoluto" },
  { label: "int", type: "function", detail: "parte inteira" },
  { label: "quad", type: "function", detail: "x²" },
  { label: "exp", type: "function", detail: "potência exp(base, e)" },
  { label: "pot", type: "function", detail: "potência pot(base, e)" },
  { label: "log", type: "function", detail: "logaritmo decimal (base 10)" },
  { label: "logn", type: "function", detail: "logaritmo natural (base e)" },
  { label: "sen", type: "function", detail: "seno em radianos" },
  { label: "cos", type: "function", detail: "cosseno em radianos" },
  { label: "tan", type: "function", detail: "tangente em radianos" },
  { label: "cotan", type: "function", detail: "cotangente em radianos" },
  { label: "arcsen", type: "function", detail: "arco seno em radianos" },
  { label: "arccos", type: "function", detail: "arco cosseno em radianos" },
  { label: "arctan", type: "function", detail: "arco tangente em radianos" },
  { label: "grauprad", type: "function", detail: "converte graus em radianos" },
  { label: "radpgrau", type: "function", detail: "converte radianos em graus" },
  { label: "pi", type: "constant", detail: "constante π" },
  { label: "rand", type: "function", detail: "real aleatório [0,1)" },
  { label: "randi", type: "function", detail: "inteiro aleatório [0,n)" },
  { label: "compr", type: "function", detail: "tamanho da string" },
  { label: "copia", type: "function", detail: "copia(texto, ini, qtd)" },
  { label: "pos", type: "function", detail: "pos(sub, texto)" },
  { label: "asc", type: "function", detail: "código do primeiro caractere" },
  { label: "carac", type: "function", detail: "caractere a partir do código" },
  { label: "maiusc", type: "function", detail: "converte para maiúsculas" },
  { label: "minusc", type: "function", detail: "converte para minúsculas" },
  { label: "numpcarac", type: "function", detail: "número para texto" },
  { label: "caracpnum", type: "function", detail: "texto para número" },
];

export function algCompletionSource(
  context: CompletionContext
): CompletionResult | null {
  const word = context.matchBefore(/[A-Za-zÀ-ÖØ-öø-ÿ_][A-Za-zÀ-ÖØ-öø-ÿ0-9_]*/);
  if (!word && !context.explicit) {
    return null;
  }
  const from = word ? word.from : context.pos;
  const typedNorm = word ? normalizeAlgWord(word.text) : "";

  const options = ALG_COMPLETIONS.filter((c) =>
    normalizeAlgWord(c.label).startsWith(typedNorm)
  );

  if (options.length === 0) return null;
  return {
    from,
    options,
    validFor: /^[A-Za-zÀ-ÖØ-öø-ÿ0-9_]*$/,
  };
}

export const algAutocompleteExtension: Extension = autocompletion({
  override: [algCompletionSource],
  activateOnTyping: true,
});

function countKeywordBalance(
  docText: string,
  openNorm: string,
  closeNorm: string
): number {
  const stripped = stripCommentsAndMaskStrings(docText);
  const words = stripped.match(/[A-Za-zÀ-ÖØ-öø-ÿ_]+/g) ?? [];
  let balance = 0;
  for (const w of words) {
    const n = normalizeAlgWord(w);
    if (n === openNorm) balance++;
    else if (n === closeNorm) balance--;
  }
  return balance;
}

export function tryAutoCloseAlgBlock(view: EditorView): boolean {
  if (completionStatus(view.state)) return false;
  const sel = view.state.selection.main;
  if (!sel.empty) return false;

  const line = view.state.doc.lineAt(sel.head);
  // Só fecha automaticamente quando cursor está no fim da linha
  if (sel.head !== line.to) return false;

  const trimmedCode = stripCommentsAndMaskStrings(line.text).trim();
  const normalizedLine = normalizeAlgWord(trimmedCode);
  const baseIndent = line.text.match(/^\s*/)?.[0] ?? "";
  const unit = "   ";
  const docText = view.state.doc.toString();

  let closingWord: string | null = null;
  if (/^se\b.*\bentao$/.test(normalizedLine)) {
    if (countKeywordBalance(docText, "se", "fimse") > 0) {
      closingWord = "fimse";
    }
  } else if (/^para\b.*\bfaca$/.test(normalizedLine)) {
    if (countKeywordBalance(docText, "para", "fimpara") > 0) {
      closingWord = "fimpara";
    }
  } else if (/^enquanto\b.*\bfaca$/.test(normalizedLine)) {
    if (countKeywordBalance(docText, "enquanto", "fimenquanto") > 0) {
      closingWord = "fimenquanto";
    }
  } else if (/^escolha\b/.test(normalizedLine)) {
    if (countKeywordBalance(docText, "escolha", "fimescolha") > 0) {
      closingWord = "fimescolha";
    }
  } else if (/\bregistro$/.test(normalizedLine)) {
    if (countKeywordBalance(docText, "registro", "fimregistro") > 0) {
      closingWord = "fimregistro";
    }
  }

  if (!closingWord) return false;

  const innerLine = `\n${baseIndent}${unit}`;
  const closerLine = `\n${baseIndent}${closingWord}`;
  const insertText = innerLine + closerLine;
  const cursorPos = sel.head + innerLine.length;

  view.dispatch({
    changes: { from: sel.head, to: sel.head, insert: insertText },
    selection: EditorSelection.cursor(cursorPos),
    scrollIntoView: true,
  });
  return true;
}

export const algAutoCloseEnterBinding: KeyBinding = {
  key: "Enter",
  run: tryAutoCloseAlgBlock,
};
