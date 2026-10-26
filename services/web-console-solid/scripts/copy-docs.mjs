import { cp, mkdir } from 'node:fs/promises';
await mkdir('dist/redoc', { recursive: true });
await cp('node_modules/swagger-ui-dist', 'dist/swagger-ui', { recursive: true });
await cp('node_modules/redoc/bundles/redoc.standalone.js', 'dist/redoc/redoc.standalone.js');
