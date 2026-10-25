import React from 'react';
import { createRoot } from 'react-dom/client';
import {
  Admin, Create, Datagrid, DateField, List, ListGuesser, Resource,
  SimpleForm, TextField, TextInput, SelectInput, BooleanInput,
} from 'react-admin';

const request = async (path, options = {}) => {
  const response = await fetch(`/api/v1/${path}`, options);
  if (!response.ok) throw new Error(await response.text());
  return response.json();
};
const dataProvider = {
  getList: async (resource) => { const data = await request(resource); return { data, total: data.length }; },
  getOne: async (resource, params) => { const data = await request(resource); return { data: data.find((item) => item.id === params.id) }; },
  getMany: async (resource) => { const data = await request(resource); return { data }; },
  getManyReference: async (resource) => { const data = await request(resource); return { data, total: data.length }; },
  create: async (resource, params) => ({ data: await request(resource, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(params.data) }) }),
  update: async () => { throw new Error('Update is not implemented yet'); },
  updateMany: async () => ({ data: [] }), delete: async () => { throw new Error('Delete is not implemented yet'); }, deleteMany: async () => ({ data: [] }),
};
const SandboxList = () => <List><Datagrid rowClick="show"><TextField source="name" /><TextField source="runtime" /><TextField source="template" /><TextField source="status" /><DateField source="created_at" showTime /></Datagrid></List>;
const SandboxCreate = () => <Create><SimpleForm><TextInput source="name" required /><SelectInput source="runtime" choices={[{ id: 'openshell', name: 'OpenShell' }, { id: 'custom', name: 'Custom' }]} defaultValue="openshell" /><SelectInput source="template" choices={[{ id: 'development', name: 'Development' }, { id: 'research', name: 'Research' }, { id: 'custom', name: 'Custom' }]} defaultValue="custom" /></SimpleForm></Create>;
const RuntimeList = () => <List><Datagrid><TextField source="name" /><TextField source="status" /><TextField source="runtime" /><TextField source="policy_id" /><TextField source="id" /></Datagrid></List>;
const PolicyList = () => <List><Datagrid rowClick="show"><TextField source="name" /><TextField source="version" /><TextField source="proxy_observation" /><TextField source="syslog_enabled" /><DateField source="created_at" showTime /></Datagrid></List>;
const PolicyCreate = () => <Create><SimpleForm><TextInput source="name" required /><BooleanInput source="proxy_observation" /><BooleanInput source="syslog_enabled" /></SimpleForm></Create>;
const ModelCreate = () => <Create><SimpleForm><TextInput source="name" required /><TextInput source="profile" required /><TextInput source="hardware_target" defaultValue="local-gpu" /></SimpleForm></Create>;
const RegistryCreate = () => <Create><SimpleForm><TextInput source="name" required /><TextInput source="url" required /><SelectInput source="kind" choices={[{ id: 'docker-hub', name: 'Docker Hub' }, { id: 'oci', name: 'OCI Registry' }, { id: 'private', name: 'Private Registry' }]} /></SimpleForm></Create>;
const App = () => <Admin dataProvider={dataProvider} title="Nestbox Control Center">
  <Resource name="sandboxes" list={SandboxList} create={SandboxCreate} />
  <Resource name="model-sandboxes" list={ListGuesser} create={ModelCreate} />
  <Resource name="policies" list={PolicyList} create={PolicyCreate} />
  <Resource name="mounts" list={ListGuesser} />
  <Resource name="registries" list={ListGuesser} create={RegistryCreate} />
  <Resource name="runtime-status" list={RuntimeList} />
  <Resource name="events" list={ListGuesser} />
  <Resource name="egress-events" list={ListGuesser} />
  <Resource name="artifacts" list={ListGuesser} />
</Admin>;
createRoot(document.getElementById('root')).render(<App />);
