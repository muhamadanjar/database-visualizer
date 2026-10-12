export async function clearWorkspace() {
  const { useEditDrafts } = await import('./editDrafts');
  useEditDrafts.setState({ drafts: {} });
  const { useWorkspace } = await import('./store');
  if (!useWorkspace.getState().tabs.length) {
    useWorkspace.setState({
      tabs: [],
      activeId: null,
      sidebarWidth: 248,
      editorHeight: 330,
    });
    return;
  }
  const { resultCache } = await import('./cache');
  const { disposeEditor } = await import('./SqlEditor');
  for (const tab of useWorkspace.getState().tabs) {
    resultCache.clear(tab.id);
    disposeEditor(tab.id);
  }
  useWorkspace.setState({
    tabs: [],
    activeId: null,
    sidebarWidth: 248,
    editorHeight: 330,
  });
}
