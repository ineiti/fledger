export function downloadFile(fileName, data) {
    console.dir(fileName);
    console.dir(data);
    const aElement = document.createElement('a');
    aElement.setAttribute('download', fileName);
    const href = URL.createObjectURL(new Blob([data]));
    aElement.href = href;
    aElement.setAttribute('target', '_blank');
    aElement.click();
    URL.revokeObjectURL(href);
};

export function getEditorContent() {
  if (typeof ace !== 'undefined' && ace.edit) {
    const editor = ace.edit("editor");
    return editor.getValue();
  }
  return "";
}