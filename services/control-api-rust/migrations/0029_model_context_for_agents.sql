-- 0029_model_context_for_agents — a window an agent framework accepts.
--
-- Hermes refuses a model whose window is below 64k ("which is below the
-- minimum 64,000 required by Hermes Agent"), and the llama.cpp row started with
-- 4,096. The window is a property of the served model, so it belongs in the row
-- the node starts from, not in a caller.
--
-- 65,536 is the smallest power-of-two window above that minimum. It costs KV
-- cache on the node, which is why it is written down here rather than picked by
-- a default.

UPDATE model_server_images
   SET command = ARRAY['-m', '{model}', '--host', '0.0.0.0', '--port', '8080',
                       '--threads', '{threads}', '--ctx-size', '65536', '--jinja']
 WHERE runtime = 'llama_cpp';

COMMENT ON COLUMN model_server_images.command IS
  'The argument vector, with {model} and {threads} as placeholders. The context window is part of the row because agent frameworks refuse a small one.';
