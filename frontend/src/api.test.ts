import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fixtureSnapshot } from '../tests/fixtures'

// Explicit automated fixtures: these never exercise or assert real provider connectivity.
beforeEach(() => { vi.resetModules(); vi.stubEnv('VITE_MODE', 'PUBLIC_SNAPSHOT') })
afterEach(() => { vi.unstubAllGlobals(); vi.unstubAllEnvs() })

function mockSnapshot(mode: unknown) {
  const snapshot = { ...fixtureSnapshot(), mode }
  const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(snapshot), { status: 200, headers: { 'Content-Type': 'application/json' } }))
  vi.stubGlobal('fetch', fetchMock)
  return { snapshot, fetchMock }
}

describe('snapshot mode integrity', () => {
  it.each(['DEMO_FIXTURE', 'INVALID_MODE', undefined, 'LOCAL_LIVE'])('rejects %s in public snapshot mode', async mode => {
    mockSnapshot(mode)
    const { loadSnapshot } = await import('./api')
    await expect(loadSnapshot()).rejects.toThrow('快照模式不匹配')
  })

  it('accepts a valid public snapshot and requests the configured base path', async () => {
    const { snapshot, fetchMock } = mockSnapshot('PUBLIC_SNAPSHOT')
    const { loadSnapshot } = await import('./api')
    await expect(loadSnapshot()).resolves.toEqual(snapshot)
    expect(fetchMock).toHaveBeenCalledWith(`${import.meta.env.BASE_URL}snapshot.json`, expect.objectContaining({ cache: 'no-store' }))
  })

  it('accepts local data only in local mode and reads the local API', async () => {
    vi.stubEnv('VITE_MODE', 'LOCAL_LIVE')
    const { snapshot, fetchMock } = mockSnapshot('LOCAL_LIVE')
    const { loadSnapshot } = await import('./api')
    await expect(loadSnapshot()).resolves.toEqual(snapshot)
    expect(fetchMock).toHaveBeenCalledWith('/api/tokens', expect.objectContaining({ cache: 'no-store' }))
  })

  it.each(['PUBLIC_SNAPSHOT', 'DEMO_FIXTURE'])('rejects %s from the local API with an explicit authenticity error', async mode => {
    vi.stubEnv('VITE_MODE', 'LOCAL_LIVE')
    mockSnapshot(mode)
    const { loadSnapshot } = await import('./api')
    await expect(loadSnapshot()).rejects.toThrow('DEMO_FIXTURE 不能作为真实数据')
  })
})

describe('local health isolation',()=>{
  it('never calls local health from public snapshot mode',async()=>{
    const fetchMock=vi.fn()
    vi.stubGlobal('fetch',fetchMock)
    const {loadLocalHealth}=await import('./api')
    await expect(loadLocalHealth()).rejects.toThrow('公开快照不访问')
    expect(fetchMock).not.toHaveBeenCalled()
  })
  it('rejects mismatched health mode even on the local endpoint',async()=>{
    vi.stubEnv('VITE_MODE','LOCAL_LIVE')
    vi.stubGlobal('fetch',vi.fn().mockResolvedValue(new Response(JSON.stringify({mode:'PUBLIC_SNAPSHOT'}))))
    const {loadLocalHealth}=await import('./api')
    await expect(loadLocalHealth()).rejects.toThrow('本地健康接口模式不匹配')
  })
})
