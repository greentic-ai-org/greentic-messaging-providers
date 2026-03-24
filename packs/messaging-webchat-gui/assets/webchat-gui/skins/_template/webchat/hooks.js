export function createStoreMiddleware() {
  return () => next => action => {
    if (action.type === 'WEB_CHAT/SEND_EVENT') {
      console.info('[template hook] sending event', action);
    }
    const result = next(action);
    if (
      action.type === 'DIRECT_LINE/INCOMING_ACTIVITY' &&
      action.payload?.activity?.from?.role === 'bot'
    ) {
      setTimeout(() => {
        window.scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' });
      }, 200);
      setTimeout(() => {
        window.scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' });
      }, 800);
    }
    return result;
  };
}

export function onBeforeRender(context) {
  console.info('[template hook] rendering tenant', context.skin.tenant);
}
