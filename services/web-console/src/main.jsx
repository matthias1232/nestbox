import React from 'react';
import { createRoot } from 'react-dom/client';
import { Admin, Resource, ListGuesser } from 'react-admin';

const request = async (path, options = {}) => {
  const response = await fetch(`/api/v1/${path}`, options);
  if (!response.ok) throw new Error(await response.text());
  return response.json();
};

const dataProvider = {
  getList: async (resource) => {
    const data = await request(resource);
    return { data, total: data.length };
  },
  getOne: async (resource, params) => {
    const data = await request(resource);
    return { data: data.find((item) => item.id === params.id) };
  },
  getMany: async (resource) => {
    const data = await request(resource);
    return { data };
  },
  getManyReference: async (resource) => {
    const data = await request(resource);
    return { data, total: data.length };
  },
  create: async (resource, params) => ({ data: await request(resource, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(params.data) }) }),
  update: async () => { throw new Error('Update is not implemented in the preview API'); },
  updateMany: async () => ({ data: [] }),
  delete: async () => { throw new Error('Delete is not implemented in the preview API'); },
  deleteMany: async () => ({ data: [] }),
};

const App = () => <Admin dataProvider={dataProvider} title="Nestbox Control Center">
  <Resource name="sandboxes" list={ListGuesser} />
  <Resource name="model-sandboxes" list={ListGuesser} />
  <Resource name="policies" list={ListGuesser} />
  <Resource name="mounts" list={ListGuesser} />
  <Resource name="registries" list={ListGuesser} />
  <Resource name="events" list={ListGuesser} />
  <Resource name="artifacts" list={ListGuesser} />
</Admin>;

createRoot(document.getElementById('root')).render(<App />);
