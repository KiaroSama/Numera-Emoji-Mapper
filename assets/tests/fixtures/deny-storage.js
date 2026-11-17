
const boom = () => { throw new DOMException('site data is blocked', 'SecurityError'); };
Object.defineProperty(window, 'localStorage', {configurable: true, get: boom});
