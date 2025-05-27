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

export function setEditorContent(data) {
  if (typeof ace !== 'undefined' && ace.edit) {
    const editor = ace.edit("editor");
    editor.setValue(data);
  }
}

// DeepSeek created code when asking to include an html string into an existing
// page, and executing existing scripts, including downloading from external
// websites.
// It creates a 'shadowRoot'
export function embedPage(data) {
  const container = document.getElementById('dht_page');
  if (!container) return;
  container.innerHTML = "";
  const wrapper = document.createElement("div");
  wrapper.className = "shadow-wrapper";
  container.appendChild(wrapper);

  const parser = new DOMParser();
  const doc = parser.parseFromString(data, 'text/html');

  const shadowRoot = wrapper.attachShadow({ mode: 'open' });

  const styles = doc.querySelectorAll('style');
  styles.forEach(style => {
      shadowRoot.appendChild(style.cloneNode(true));
  });

  const body = doc.querySelector('body');
  if (body) {
      const bodyClone = body.cloneNode(true);
      const scripts = bodyClone.querySelectorAll('script');
      scripts.forEach(script => script.remove());
      shadowRoot.appendChild(bodyClone);
  }

  const allScripts = doc.querySelectorAll('script');
  allScripts.forEach(originalScript => {
      if (originalScript.src) {
          // Handle external scripts
          fetch(originalScript.src)
              .then(response => response.text())
              .then(scriptContent => {
                  const rewrittenContent = rewriteSelectors(scriptContent);
                  const script = document.createElement('script');
                  script.textContent = rewrittenContent;
                  console.log("Appending rewritten external script", script);
                  shadowRoot.appendChild(script);
              })
              .catch(error => console.error("Failed to fetch external script:", error));
      } else {
          // Handle inline scripts
          const script = document.createElement('script');

          script.textContent = `
          documentRoot = document.getElementById("dht_page").firstChild.shadowRoot;
          ${rewriteSelectors(originalScript.textContent)}
          `;

          console.log(script);
          shadowRoot.appendChild(script);
      }
  });
}

function rewriteSelectors(script) {
  return script
      .replace(/document\.getElementById\(/g, 'documentRoot.getElementById(')
      .replace(/document\.querySelector\(/g, 'documentRoot.querySelector(')
      .replace(/document\.querySelectorAll\(/g, 'documentRoot.querySelectorAll(');
}
