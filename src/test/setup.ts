import "@testing-library/jest-dom/vitest";

// Polyfill mínimo para medição de texto do CodeMirror 6 no jsdom
if (typeof Range !== "undefined") {
  if (!Range.prototype.getClientRects) {
    Range.prototype.getClientRects = function () {
      return {
        length: 0,
        item: () => null,
        [Symbol.iterator]: function* () {},
      } as unknown as DOMRectList;
    };
  }
  if (!Range.prototype.getBoundingClientRect) {
    Range.prototype.getBoundingClientRect = function () {
      return {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
        toJSON: () => ({}),
      } as DOMRect;
    };
  }
}
