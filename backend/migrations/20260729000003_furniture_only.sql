-- Restrict the catalogue to furniture. Lighting fixtures and soft
-- furnishings/decor are not furniture and are out of scope for the buyer's app,
-- so the seed rows for those categories are removed.
--
-- 20260729000002 is already applied in existing databases and its checksum is
-- validated on boot, so it cannot be edited in place; this migration corrects
-- the seed instead.
--
-- The NOT EXISTS guard means the delete cannot fail against a database where
-- one of these products was already ordered. A referenced product is left in
-- place deliberately: order history must keep pointing at a real row, or the
-- recorded line loses its product name and SKU.
DELETE FROM products
WHERE category IN ('Lighting', 'Accessories')
  AND NOT EXISTS (
      SELECT 1 FROM order_items WHERE order_items.product_id = products.id
  );
