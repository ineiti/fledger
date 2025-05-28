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

  // Handle external scripts
  const allScripts = doc.querySelectorAll('script');
  const externalScripts = [];
  allScripts.forEach(originalScript => {
    if (originalScript.src) {
      externalScripts.push(fetch(originalScript.src)
        .then(response => response.text())
        .then(scriptContent => {
          appendScript(shadowRoot, scriptContent);
        })
        .catch(error => console.error("Failed to fetch external script:", error)));
    }
  });

  // Wait for external scripts to load, or timeout.
  Promise.race([
    Promise.all(externalScripts),
    new Promise((_, reject) => {
      setTimeout(() => reject(new Error("Timeout while loading scripts")), 1000);
    })]
  ).then(() => {
    // Finally handle inline scripts
    allScripts.forEach(originalScript => {
      if (originalScript.src.length == 0) {
        appendScript(shadowRoot, originalScript.textContent);
      }
    });
  }).catch((e) => {
    alert(`Couldn't load scripts: ${e}`);
  })
}

function appendScript(shadowRoot, source) {
  const rewritten = source
    .replace(/document\.getElementById\(/g, 'documentRoot.getElementById(')
    .replace(/document\.querySelector\(/g, 'documentRoot.querySelector(')
    .replace(/document\.querySelectorAll\(/g, 'documentRoot.querySelectorAll(');

  const script = document.createElement('script');
  script.textContent = `
      documentRoot = document.getElementById("dht_page").firstChild.shadowRoot;
      ${rewritten}
      `;
  shadowRoot.appendChild(script);
}
