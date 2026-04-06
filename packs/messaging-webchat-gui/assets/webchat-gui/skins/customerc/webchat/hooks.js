export function createStoreMiddleware() {
  const seenActivityIds = new Set();
  let lastWatermark = -1;
  return () => next => action => {
    console.log('[hooks] action:', action.type, action.payload?.activity?.id ?? '', action);

    // Block duplicate INCOMING_ACTIVITY by ID
    if (action.type === 'DIRECT_LINE/INCOMING_ACTIVITY') {
      const activity = action.payload?.activity;
      const id = activity?.id;

      console.log('[hooks] INCOMING_ACTIVITY id:', id, 'channelData:', activity?.channelData);

      if (id && seenActivityIds.has(id)) {
        console.log('[hooks] BLOCKED duplicate activity:', id);
        return;
      }
      if (id) {
        seenActivityIds.add(id);
      }
    }

    // Block RECEIVE_DIRECT_LINE_ACTIVITIES if watermark unchanged
    if (action.type === 'DIRECT_LINE/RECEIVE_ACTIVITIES') {
      const watermark = parseInt(action.payload?.watermark ?? '-1', 10);
      const activities = action.payload?.activities ?? [];
      console.log('[hooks] RECEIVE_ACTIVITIES watermark:', watermark, 'lastWatermark:', lastWatermark, 'count:', activities.length);

      if (watermark <= lastWatermark && activities.length === 0) {
        console.log('[hooks] BLOCKED empty poll with same/old watermark');
        return;
      }
      lastWatermark = watermark;
    }

    return next(action);
  };
}

export function onBeforeRender(context) {
  console.info('[hooks] rendering tenant', context.skin.tenant);
}
