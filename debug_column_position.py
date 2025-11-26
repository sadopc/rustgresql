query = "SELECT 'High Earners' AS category, COUNT(*) AS count FROM employees WHERE salary > 80000.00 UNION ALL SELECT 'Low Earners' AS category, COUNT(*) AS count FROM employees WHERE salary <= 60000.00 UNION ALL SELECT 'Mid Range' AS category, COUNT(*) AS count FROM employees WHERE salary > 60000.00 AND salary <= 80000.00;"

print("Query length:", len(query))
print("Character at position 47 (index 47):", repr(query[47]))
print("Character at position 48 (index 48):", repr(query[48]))
print("Character at position 49 (index 49):", repr(query[49]))

# Show context around position 48
start = max(0, 48 - 10)
end = min(len(query), 48 + 10)
print("\nContext around position 48:")
print(f"Position: {''.join([str(i%10) for i in range(start, end)])}")
print(f"Chars:    {repr(query[start:end])}")
print(f"          {' ' * (48 - start)}^")