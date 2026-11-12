import { useCallback, useEffect, useRef, useState } from "react";
import { CLASSIC_ALGORITHM_SKELETON } from "../data/skeleton";
import type { IdeBridge } from "../services/tauriBridge";
import type { AlgorithmExample, SourceEncoding } from "../types/ide";
import { getFileName, getParentDir } from "../utils/pathUtils";
import { getRecoverySource, setRecoverySource } from "../utils/recoveryDraft";
import { normalizeLineEndings, suggestFileName } from "../utils/algoritmosText";

export interface DocumentSnapshot {
  source: string;
  filePath: string | null;
  manualWorkingDir: string | null;
  docRevision: number;
}

interface UseIdeDocumentOptions {
  bridge: IdeBridge;
  defaultSaveEncoding: SourceEncoding;
  onBeforeReplaceDocument: () => Promise<void>;
  onFileError: (message: string) => void;
}

export function useIdeDocument({
  bridge,
  defaultSaveEncoding,
  onBeforeReplaceDocument,
  onFileError,
}: UseIdeDocumentOptions) {
  const [filePath, setFilePath] = useState<string | null>(null);
  const [manualWorkingDir, setManualWorkingDir] = useState<string | null>(null);
  const [fileName, setFileName] = useState("semnome.alg");
  const [fileEncoding, setFileEncoding] =
    useState<SourceEncoding>(defaultSaveEncoding);
  const [source, setSourceState] = useState(CLASSIC_ALGORITHM_SKELETON);
  const [savedSource, setSavedSource] = useState(CLASSIC_ALGORITHM_SKELETON);
  const [docResetKey, setDocResetKey] = useState(0);
  const [pendingRecoveryDraft, setPendingRecoveryDraft] = useState<
    string | null
  >(() => {
    const saved = getRecoverySource();
    return saved && saved.length > 0 ? saved : null;
  });

  const encoding: SourceEncoding =
    filePath === null ? defaultSaveEncoding : fileEncoding;
  const authorizedSandboxDir: string | null =
    manualWorkingDir ?? (filePath ? getParentDir(filePath) : null);

  const isDirty = source !== savedSource;
  const isDirtyRef = useRef(isDirty);
  isDirtyRef.current = isDirty;

  const sourceRef = useRef(source);
  sourceRef.current = source;

  const savedSourceRef = useRef(savedSource);
  savedSourceRef.current = savedSource;

  const filePathRef = useRef(filePath);
  filePathRef.current = filePath;

  const manualWorkingDirRef = useRef(manualWorkingDir);
  manualWorkingDirRef.current = manualWorkingDir;

  const pendingRecoveryDraftRef = useRef(pendingRecoveryDraft);
  pendingRecoveryDraftRef.current = pendingRecoveryDraft;

  const docRevisionRef = useRef(1);
  const docIdentityRef = useRef(1);

  useEffect(() => {
    const title = `${fileName}${isDirty ? " *" : ""} — AlgoritmosLab`;
    void bridge.setWindowTitle(title);
  }, [bridge, fileName, isDirty]);

  const setSource = useCallback((next: string) => {
    docRevisionRef.current += 1;
    sourceRef.current = next;
    setSourceState(next);
    pendingRecoveryDraftRef.current = null;
    setPendingRecoveryDraft(null);
    setRecoverySource(next !== savedSourceRef.current ? next : null);
  }, []);

  const restoreRecoveryDraft = useCallback(async () => {
    const draft = pendingRecoveryDraftRef.current;
    if (draft === null) return;
    const startIdentity = docIdentityRef.current;
    await onBeforeReplaceDocument();
    if (docIdentityRef.current !== startIdentity) return;

    const normalizedDraft = normalizeLineEndings(draft);
    docIdentityRef.current += 1;
    docRevisionRef.current += 1;
    sourceRef.current = normalizedDraft;
    setSourceState(normalizedDraft);
    pendingRecoveryDraftRef.current = null;
    setPendingRecoveryDraft(null);
    setRecoverySource(
      normalizedDraft !== savedSourceRef.current ? normalizedDraft : null
    );
    setDocResetKey((k) => k + 1);
  }, [onBeforeReplaceDocument]);

  const dismissRecoveryDraft = useCallback(() => {
    pendingRecoveryDraftRef.current = null;
    setPendingRecoveryDraft(null);
    setRecoverySource(null);
  }, []);

  const getSnapshot = useCallback((): DocumentSnapshot => {
    return {
      source: sourceRef.current,
      filePath: filePathRef.current,
      manualWorkingDir: manualWorkingDirRef.current,
      docRevision: docRevisionRef.current,
    };
  }, []);

  const confirmDiscardIfDirty = useCallback(async (): Promise<boolean> => {
    if (!isDirtyRef.current) return true;
    return bridge.confirmAction(
      "Há alterações não salvas no algoritmo atual. Deseja descartá-las?",
      "Alterações não salvas"
    );
  }, [bridge]);

  const applyReplacedDocument = useCallback(
    (next: {
      filePath: string | null;
      fileName: string;
      encoding: SourceEncoding;
      text: string;
    }) => {
      docIdentityRef.current += 1;
      docRevisionRef.current += 1;
      filePathRef.current = next.filePath;
      manualWorkingDirRef.current = null;
      sourceRef.current = next.text;
      savedSourceRef.current = next.text;
      pendingRecoveryDraftRef.current = null;
      setFilePath(next.filePath);
      setManualWorkingDir(null);
      setFileName(next.fileName);
      setFileEncoding(next.encoding);
      setSourceState(next.text);
      setSavedSource(next.text);
      setPendingRecoveryDraft(null);
      setRecoverySource(null);
      setDocResetKey((k) => k + 1);
    },
    []
  );

  const handleNew = useCallback(async () => {
    if (!(await confirmDiscardIfDirty())) return;
    const startIdentity = docIdentityRef.current;
    await onBeforeReplaceDocument();
    if (docIdentityRef.current !== startIdentity) return;
    applyReplacedDocument({
      filePath: null,
      fileName: "semnome.alg",
      encoding: defaultSaveEncoding,
      text: CLASSIC_ALGORITHM_SKELETON,
    });
  }, [
    applyReplacedDocument,
    confirmDiscardIfDirty,
    defaultSaveEncoding,
    onBeforeReplaceDocument,
  ]);

  const handleOpen = useCallback(async () => {
    if (!(await confirmDiscardIfDirty())) return;
    const startIdentity = docIdentityRef.current;
    const picked = await bridge.pickOpenAlgPath();
    if (!picked || docIdentityRef.current !== startIdentity) return;

    try {
      const opened = await bridge.openAlgFile(picked);
      if (docIdentityRef.current !== startIdentity) return;
      await onBeforeReplaceDocument();
      if (docIdentityRef.current !== startIdentity) return;
      applyReplacedDocument({
        filePath: opened.path,
        fileName: opened.name,
        encoding: opened.encoding,
        text: normalizeLineEndings(opened.text),
      });
    } catch (err) {
      if (docIdentityRef.current === startIdentity) {
        onFileError(String(err));
      }
    }
  }, [
    applyReplacedDocument,
    bridge,
    confirmDiscardIfDirty,
    onBeforeReplaceDocument,
    onFileError,
  ]);

  const performSave = useCallback(
    async (targetEncoding: SourceEncoding, forceDialog: boolean) => {
      const startIdentity = docIdentityRef.current;
      const initialSource = sourceRef.current;
      let destPath = forceDialog ? null : filePathRef.current;
      if (!destPath) {
        const defaultName =
          filePathRef.current ??
          suggestFileName(initialSource, fileName || "algoritmo.alg");
        destPath = await bridge.pickSaveAlgPath(defaultName);
        if (docIdentityRef.current !== startIdentity) return;
        if (!destPath) return;
      }

      const saveIdentity = docIdentityRef.current;
      const textToSave = sourceRef.current;
      try {
        await bridge.saveAlgFile(destPath, textToSave, targetEncoding);
        if (docIdentityRef.current !== saveIdentity) return;
        const extractedName = getFileName(destPath, fileName);
        const normalizedSaved = normalizeLineEndings(textToSave);
        filePathRef.current = destPath;
        savedSourceRef.current = normalizedSaved;
        setFilePath(destPath);
        setFileName(extractedName);
        setFileEncoding(targetEncoding);
        setSavedSource(normalizedSaved);
        if (sourceRef.current === normalizedSaved) {
          setRecoverySource(null);
        }
      } catch (err) {
        if (docIdentityRef.current === saveIdentity) {
          onFileError(`Falha ao salvar arquivo: ${String(err)}`);
        }
      }
    },
    [bridge, fileName, onFileError]
  );

  const handleSave = useCallback(async () => {
    const targetEncoding =
      filePathRef.current !== null ? encoding : defaultSaveEncoding;
    await performSave(targetEncoding, false);
  }, [defaultSaveEncoding, encoding, performSave]);

  const handleSaveAsWindows1252 = useCallback(async () => {
    await performSave("windows1252", true);
  }, [performSave]);

  const loadExample = useCallback(
    async (example: AlgorithmExample) => {
      if (!(await confirmDiscardIfDirty())) return;
      const startIdentity = docIdentityRef.current;
      await onBeforeReplaceDocument();
      if (docIdentityRef.current !== startIdentity) return;
      applyReplacedDocument({
        filePath: null,
        fileName: `${example.id}.alg`,
        encoding: defaultSaveEncoding,
        text: normalizeLineEndings(example.source),
      });
    },
    [
      applyReplacedDocument,
      confirmDiscardIfDirty,
      defaultSaveEncoding,
      onBeforeReplaceDocument,
    ]
  );

  const handleSelectWorkingDir = useCallback(async () => {
    const startIdentity = docIdentityRef.current;
    const picked = await bridge.pickWorkingDir();
    if (!picked || docIdentityRef.current !== startIdentity) return;
    manualWorkingDirRef.current = picked;
    setManualWorkingDir(picked);
  }, [bridge]);

  return {
    filePath,
    manualWorkingDir,
    authorizedSandboxDir,
    fileName,
    encoding,
    source,
    isDirty,
    isDirtyRef,
    docResetKey,
    pendingRecoveryDraft,
    setSource,
    setManualWorkingDir,
    getSnapshot,
    handleNew,
    handleOpen,
    handleSave,
    handleSaveAsWindows1252,
    loadExample,
    handleSelectWorkingDir,
    restoreRecoveryDraft,
    dismissRecoveryDraft,
  };
}
