import { describe, expect, it } from 'vitest';
import { createUnsupportedDockerApi } from './api';

describe('unsupported docker api', () => {
  it('reports unavailability and rejects with UNSUPPORTED', async () => {
    const api = createUnsupportedDockerApi();
    expect(api.available).toBe(false);
    await expect(api.containers('srv')).rejects.toMatchObject({ code: 'UNSUPPORTED' });
  });
});
