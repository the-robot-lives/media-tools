# Lit Component Generation (Default)

Use this template for `type: component` unless the user explicitly requests React, JSX/TSX, or a React-specific component contract. `type: react-page` remains explicitly React. A generic request for a component does not imply React.

## System Prompt

```text
You generate Lit web components. Output one TypeScript module importing LitElement, html and css from lit. Extend LitElement and register a descriptive hyphenated custom-element name. Declare reactive properties, use scoped styles and accessible semantic HTML, and expose changes through bubbling, composed CustomEvents when needed. Use static properties and customElements.define so decorators are not required. Return only valid TypeScript source, without explanations or Markdown fences. Use React only when explicitly requested.
```

## Example .media.prompt

```yaml
schema: "0.4"
id: lit-button
type: component
quality: medium

prompt:
  system: |
    Generate a Lit web component as one TypeScript module. Import LitElement,
    html and css from lit, declare static reactive properties, and register
    app-button with customElements.define. Use scoped styles and accessible
    semantic HTML. Output source only, without Markdown fences.
  text: |
    Create an accessible button with primary and secondary variants,
    an optional disabled property, a label slot and an app-activate CustomEvent.

output:
  formats:
    - format: ts
  text_format: ts

tags: [lit, component, button]
```

The configured chat provider/profile supplies the model. The default artifact is `.ts`; explicitly requested JavaScript uses `text_format: js` and `.js`.

## Expected Module Shape

```typescript
import {LitElement, html, css} from 'lit';

export class AppButton extends LitElement {
  static properties = {
    variant: {type: String, reflect: true},
    disabled: {type: Boolean, reflect: true},
  };

  declare variant: string;
  declare disabled: boolean;

  constructor() {
    super();
    this.variant = 'primary';
    this.disabled = false;
  }

  static styles = css`
    :host { display: inline-block; }
    button { padding: .65rem 1rem; border-radius: .4rem; }
    :host([variant="primary"]) button { background: #164e63; color: white; }
    :host([variant="secondary"]) button { background: white; color: #164e63; }
    button:focus-visible { outline: 3px solid #0e7490; outline-offset: 2px; }
  `;

  render() {
    return html`<button ?disabled=${this.disabled} @click=${this.activate}>
      <slot></slot>
    </button>`;
  }

  private activate() {
    this.dispatchEvent(new CustomEvent('app-activate', {
      bubbles: true,
      composed: true,
    }));
  }
}

customElements.define('app-button', AppButton);
```

Use `declare` for typed reactive fields with `static properties`; emitted class fields must not shadow Lit's generated accessors. Build TypeScript with the host application's toolchain and import the module before mounting `<app-button>Save</app-button>`.

## Explicit React Override

For “create a React button component,” use the [React component template](react-component.md), explicitly request React in the prompt contract, and set `text_format: tsx` (or `jsx`). Do not wrap the default Lit element in React unless a wrapper is requested.
