use rustgresql::optimizer::OptimizedQueryPlanner;
use rustgresql::sql::ast::*;
use rustgresql::executor::planner::PlanNode;

#[test]
fn test_plan_limit_and_order_by() {
    let planner = OptimizedQueryPlanner::new();

    // SELECT * FROM t1 ORDER BY col1 DESC LIMIT 5
    let select = SelectStatement::Simple {
        with_clause: None,
        distinct: false,
        columns: vec![ColumnSpec {
            expr: Expression::Star,
            alias: None,
        }],
        from: vec![TableRef { name: "t1".to_string(), alias: None }],
        joins: vec![],
        where_clause: None,
        group_by: vec![],
        having: None,
        order_by: vec![OrderBy {
            expr: Expression::Column { name: "col1".to_string(), table: None },
            direction: SortDirection::Desc,
        }],
        limit: Some(5),
        offset: None,
        named_windows: vec![],
    };

    let indexes = vec![];
    let plan = planner.plan_select(&select, &indexes).expect("Planning failed");

    // Verify plan structure
    // Expected: Limit -> Sort -> Project -> Scan (or similar, Project/Scan might be swapped/merged depending on optimization)
    // The key is that Limit and Sort nodes MUST exist.

    // 1. Top node should be Limit
    if let PlanNode::Limit { input, limit, offset } = plan.root {
        assert_eq!(limit, 5);
        assert_eq!(offset, None);

        // 2. Child of Limit should be Sort
        if let PlanNode::Sort { input: sort_input, order_by } = *input {
            assert_eq!(order_by.len(), 1);
            assert!(matches!(order_by[0].direction, SortDirection::Desc));

            // 3. Child of Sort should be Project (or Scan if optimized, but usually Project adds columns)
            // Depending on implementation, might be Project or directly Scan if SELECT *
            // But in OptimizedQueryPlanner::plan_select:
            // - it applies joins/scan
            // - then filter
            // - then aggregation
            // - then projection (Project)
            // - then Sort
            // - then Limit
            // So we expect Project under Sort.

            // Let's check if it is a Project or Scan, just to be sure we traversed correctly
            match *sort_input {
                PlanNode::Project { .. } => {}, // OK
                PlanNode::Scan { .. } => {}, // OK (if no projection needed, though logic usually adds one for Star expansion or explicit columns)
                _ => panic!("Expected Project or Scan under Sort, got {:?}", sort_input),
            }

        } else {
            panic!("Expected Sort under Limit, got {:?}", input);
        }

    } else {
        panic!("Expected Limit at root, got {:?}", plan.root);
    }
}

#[test]
fn test_plan_limit_only() {
    let planner = OptimizedQueryPlanner::new();

    // SELECT * FROM t1 LIMIT 10
    let select = SelectStatement::Simple {
        with_clause: None,
        distinct: false,
        columns: vec![ColumnSpec {
            expr: Expression::Star,
            alias: None,
        }],
        from: vec![TableRef { name: "t1".to_string(), alias: None }],
        joins: vec![],
        where_clause: None,
        group_by: vec![],
        having: None,
        order_by: vec![],
        limit: Some(10),
        offset: Some(2),
        named_windows: vec![],
    };

    let indexes = vec![];
    let plan = planner.plan_select(&select, &indexes).expect("Planning failed");

    // Verify plan structure
    // Top node should be Limit
    if let PlanNode::Limit { input: _, limit, offset } = plan.root {
        assert_eq!(limit, 10);
        assert_eq!(offset, Some(2));
    } else {
        panic!("Expected Limit at root, got {:?}", plan.root);
    }
}
