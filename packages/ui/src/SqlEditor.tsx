import { useEffect, useRef } from 'react';
import Editor, { loader, type OnMount } from '@monaco-editor/react';
import * as monaco from 'monaco-editor/editor/editor.api.js';
import 'monaco-editor/languages/definitions/sql/register.js';
import EditorWorker from 'monaco-editor/editor/editor.worker.js?worker';
import { format } from 'sql-formatter';
import type { Engine } from '@database-studio/contracts';
import type { Tab } from './store';
import { completions as contextualCompletions } from './completion';

(
  globalThis as typeof globalThis & { MonacoEnvironment: unknown }
).MonacoEnvironment = { getWorker: () => new EditorWorker() };
loader.config({ monaco });
export interface EditorHandle {
  selection: () => string;
  cursor: () => number;
  format: () => void;
}
export function disposeEditor(id: string) {
  monaco.editor
    .getModel(monaco.Uri.parse('studio://editor/' + id + '.sql'))
    ?.dispose();
}
export default function SqlEditor({
  tab,
  engine,
  onChange,
  onReady,
  completions,
}: {
  tab: Tab;
  engine: Engine;
  onChange: (sql: string) => void;
  onReady: (handle: EditorHandle) => void;
  completions: string[];
}) {
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const names = useRef(completions);
  names.current = completions;
  useEffect(() => {
    const provider = monaco.languages.registerCompletionItemProvider('sql', {
      provideCompletionItems: (model, position) => {
        if (
          model.uri.toString() !==
          monaco.Uri.parse('studio://editor/' + tab.id + '.sql').toString()
        )
          return { suggestions: [] };
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };
        return {
          suggestions: contextualCompletions(
            model.getValue(),
            model.getOffsetAt(position),
            engine,
            names.current,
          ).map((label) => ({
            label,
            kind: monaco.languages.CompletionItemKind.Field,
            insertText: label,
            range,
          })),
        };
      },
    });
    return () => provider.dispose();
  }, [tab.id, engine]);
  const mount: OnMount = (instance) => {
    editor.current = instance;
    if (import.meta.env.VITE_DESKTOP_E2E === '1') {
      (
        globalThis as typeof globalThis & {
          __DATABASE_STUDIO_SET_SQL__?: (sql: string) => void;
        }
      ).__DATABASE_STUDIO_SET_SQL__ = (sql) => instance.setValue(sql);
    }
    onReady({
      selection: () => {
        const selection = instance.getSelection();
        return selection
          ? (instance.getModel()?.getValueInRange(selection) ?? '')
          : '';
      },
      cursor: () => {
        const position = instance.getPosition();
        return position ? (instance.getModel()?.getOffsetAt(position) ?? 0) : 0;
      },
      format: () => {
        const model = instance.getModel();
        if (model)
          instance.executeEdits('format', [
            {
              range: model.getFullModelRange(),
              text: format(model.getValue(), {
                language: engine === 'Postgres' ? 'postgresql' : 'mysql',
              }),
            },
          ]);
      },
    });
  };
  return (
    <Editor
      path={`studio://editor/${tab.id}.sql`}
      language="sql"
      value={tab.sql}
      onChange={(value) => onChange(value ?? '')}
      onMount={mount}
      saveViewState
      keepCurrentModel
      options={{
        fontSize: 13,
        fontFamily: '"Cascadia Code", "SFMono-Regular", Consolas, monospace',
        minimap: { enabled: false },
        lineNumbers: 'on',
        scrollBeyondLastLine: false,
        automaticLayout: true,
        tabSize: 2,
        wordWrap: 'off',
        padding: { top: 18 },
        renderLineHighlight: 'gutter',
        stickyScroll: { enabled: false },
      }}
    />
  );
}
